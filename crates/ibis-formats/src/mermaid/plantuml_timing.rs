// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust native SVG renderer for UML 2.x PlantUML Timing diagrams
//! (`robust`, `concise`, `clock`, `binary`, `analog`, `@t` / `@+dt`, cross-participant
//! messages, duration constraints, and highlight bands).

use std::collections::HashMap;

use super::plantuml::{
    PlantUmlNativeRender, PlantUmlUnsupported, excerpt, sanitize_label, split_as_outside_quotes,
    split_colon_outside_quotes, split_trailing_color, strip_block_comments, strip_prefix_ci,
    strip_quotes,
};
use super::plantuml_uml::{UmlSkip, advance_skip, handle_common_ignored_or_unsupported, safe_id};
use super::{MermaidOutput, escape_xml as svg_escape, estimate_text_width};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimingKind {
    Robust,
    Concise,
    Binary,
    Clock,
    Analog,
}

struct TimingParticipant {
    label: String,
    kind: TimingKind,
    clock_period: Option<f64>,
    /// Ordered unique state names for `Robust` participants (bottom-to-top or discovery order).
    states: Vec<String>,
    /// `(time, state_label)` transitions sorted by time.
    changes: Vec<(f64, String)>,
}

struct TimingMessage {
    from_idx: usize,
    from_time: f64,
    to_idx: usize,
    to_time: f64,
    label: String,
}

struct TimingConstraint {
    participant_idx: usize,
    t1: f64,
    t2: f64,
    label: String,
}

struct TimingHighlight {
    t1: f64,
    t2: f64,
    color: String,
    label: Option<String>,
}

pub(super) fn render_plantuml_timing_native(source: &str) -> PlantUmlNativeRender {
    let mut unsupported = Vec::new();
    let mut title: Option<String> = None;
    let mut participants: Vec<TimingParticipant> = Vec::new();
    let mut alias_map: HashMap<String, usize> = HashMap::new();
    let mut messages: Vec<TimingMessage> = Vec::new();
    let mut constraints: Vec<TimingConstraint> = Vec::new();
    let mut highlights: Vec<TimingHighlight> = Vec::new();
    let mut tick_labels: Vec<(f64, String)> = Vec::new();

    let mut current_time: f64 = 0.0;
    let mut saw_any_time = false;
    let mut named_times: HashMap<String, f64> = HashMap::new();
    let mut next_symbolic_time: f64 = 0.0;
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_enduml = false;

    let register_tick = |t: f64, lbl: String, ticks: &mut Vec<(f64, String)>| {
        if !ticks.iter().any(|(et, _)| (*et - t).abs() < 1e-6) {
            ticks.push((t, lbl));
        }
    };

    for (idx, raw) in source.lines().enumerate() {
        let line_no = idx + 1;
        if saw_enduml {
            continue;
        }
        let cleaned = strip_block_comments(raw, &mut in_comment);
        let trimmed = cleaned.trim();
        if advance_skip(trimmed, &mut skip_until) {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('\'') {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("@start") {
            continue;
        }
        if lower.starts_with("@end") {
            saw_enduml = true;
            continue;
        }
        if handle_common_ignored_or_unsupported(
            line_no,
            trimmed,
            &lower,
            &mut skip_until,
            &mut unsupported,
        ) {
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "title ") {
            let t = sanitize_label(rest);
            if !t.is_empty() {
                title = Some(t);
            }
            continue;
        }
        if lower.starts_with("mode ") {
            continue;
        }
        // Participant declaration: `robust "Web" as W`, `concise ...`, `binary ...`, `clock ...`, `analog ...`
        if let Some((kind, rest)) = [
            ("robust ", TimingKind::Robust),
            ("concise ", TimingKind::Concise),
            ("binary ", TimingKind::Binary),
            ("clock ", TimingKind::Clock),
            ("analog ", TimingKind::Analog),
        ]
        .iter()
        .find_map(|&(kw, k)| strip_prefix_ci(trimmed, kw).map(|r| (k, r)))
        {
            let mut body = rest.trim();
            let mut clock_period = None;
            if kind == TimingKind::Clock
                && let Some(pos) = body.to_ascii_lowercase().find(" with period ")
            {
                let after = body[pos + " with period ".len()..].trim();
                clock_period = after
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|&p| p > 0.0);
                body = body[..pos].trim();
            }
            let (clean, _) = split_trailing_color(body);
            let (id, label) = if let Some((lhs, rhs)) = split_as_outside_quotes(clean) {
                let lhs = sanitize_label(strip_quotes(lhs.trim()));
                let rhs = safe_id(rhs.trim());
                (rhs, lhs)
            } else {
                let lbl = sanitize_label(strip_quotes(clean));
                (safe_id(&lbl), lbl)
            };
            let p_idx = participants.len();
            alias_map.insert(id.clone(), p_idx);
            alias_map.insert(label.clone(), p_idx);
            participants.push(TimingParticipant {
                label,
                kind,
                clock_period,
                states: Vec::new(),
                changes: Vec::new(),
            });
            continue;
        }
        // Highlight band: `highlight 100 to 200 #Gold : Wait`
        if let Some(rest) = strip_prefix_ci(trimmed, "highlight ") {
            let (expr, lbl) = match split_colon_outside_quotes(rest) {
                Some((e, l)) => (e.trim(), Some(sanitize_label(l))),
                None => (rest.trim(), None),
            };
            let (expr_no_col, _) = split_trailing_color(expr);
            let color = extract_color_token(expr).unwrap_or_else(|| "#fef3c7".to_string());
            if let Some((a, b)) = split_word_ci(expr_no_col, "to")
                && let (Some(t1), Some(t2)) = (parse_time_val(a, current_time), parse_time_val(b, current_time))
            {
                highlights.push(TimingHighlight {
                    t1: t1.min(t2),
                    t2: t1.max(t2),
                    color,
                    label: lbl.filter(|s| !s.is_empty()),
                });
                continue;
            }
        }
        // Timestamp line: `@0`, `@100`, `@+50`, `@2026-04-01`, `@0 as :t0`
        if let Some(after_at) = trimmed.strip_prefix('@') {
            let (time_part, alias_part) = match split_as_outside_quotes(after_at) {
                Some((t, a)) => (t.trim(), Some(a.trim())),
                None => (after_at.trim(), None),
            };
            // Check if this is `@t1 <-> @t2 : {label}` attached to the last participant.
            if trimmed.contains("<->")
                && let Some(p_idx) = participants.len().checked_sub(1)
                && let Some(c) = parse_constraint_line(trimmed, p_idx, current_time, &named_times)
            {
                constraints.push(c);
                continue;
            }
            if let Some(t) = parse_time_val(time_part, current_time) {
                current_time = t;
                saw_any_time = true;
                register_tick(t, format_time_tick(t), &mut tick_labels);
                if let Some(alias) = alias_part {
                    named_times.insert(alias.to_string(), t);
                }
                continue;
            }
            if let Some(&t) = named_times.get(time_part) {
                current_time = t;
                saw_any_time = true;
                continue;
            }
            // Symbolic / date timestamp (e.g. `@2026-04-01`)
            let t = next_symbolic_time;
            next_symbolic_time += 100.0;
            current_time = t;
            saw_any_time = true;
            named_times.insert(time_part.to_string(), t);
            register_tick(t, time_part.to_string(), &mut tick_labels);
            if let Some(alias) = alias_part {
                named_times.insert(alias.to_string(), t);
            }
            continue;
        }
        // Duration constraint on a named participant: `WB@100 <-> @300 : {200 ms}`
        if trimmed.contains("<->")
            && let Some((head, _)) = trimmed.split_once('@')
            && let Some(&p_idx) = alias_map.get(head.trim())
            && let Some(c) = parse_constraint_line(
                &trimmed[head.len()..],
                p_idx,
                current_time,
                &named_times,
            )
        {
            constraints.push(c);
            continue;
        }
        // Cross-participant message: `WU -> WB : URL` or `WB -> WU@+50 : OK`
        if let Some((lhs_raw, rhs_raw, msg_lbl)) = split_timing_message(trimmed) {
            let (from_name, from_off) = split_target_time(lhs_raw);
            let (to_name, to_off) = split_target_time(rhs_raw);
            if let (Some(&from_idx), Some(&to_idx)) =
                (alias_map.get(from_name), alias_map.get(to_name))
            {
                let from_time = from_off
                    .and_then(|o| resolve_time_ref(o, current_time, &named_times))
                    .unwrap_or(current_time);
                let to_time = to_off
                    .and_then(|o| resolve_time_ref(o, from_time, &named_times))
                    .unwrap_or(from_time);
                register_tick(from_time, format_time_tick(from_time), &mut tick_labels);
                register_tick(to_time, format_time_tick(to_time), &mut tick_labels);
                messages.push(TimingMessage {
                    from_idx,
                    from_time,
                    to_idx,
                    to_time,
                    label: msg_lbl.map(sanitize_label).unwrap_or_default(),
                });
                continue;
            }
        }
        // State transition: `WB is Idle` / `WB is "Waiting"` / `WB@100 is Processing`
        if let Some((lhs_raw, state_raw)) = split_word_ci(trimmed, "is") {
            let (p_name, t_ref) = split_target_time(lhs_raw.trim());
            if let Some(&p_idx) = alias_map.get(p_name) {
                let t = t_ref
                    .and_then(|r| resolve_time_ref(r, current_time, &named_times))
                    .unwrap_or(current_time);
                if !saw_any_time {
                    saw_any_time = true;
                    register_tick(t, format_time_tick(t), &mut tick_labels);
                }
                let (state_no_col, _) = split_trailing_color(state_raw.trim());
                let state_clean = sanitize_label(strip_quotes(state_no_col.trim()));
                let p = &mut participants[p_idx];
                if p.kind == TimingKind::Robust
                    && !state_clean.is_empty()
                    && !matches!(state_clean.as_str(), "{hidden}" | "{-}")
                    && !p.states.contains(&state_clean)
                {
                    p.states.push(state_clean.clone());
                }
                p.changes.push((t, state_clean));
                continue;
            }
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported PlantUML timing diagram syntax.".to_string(),
        });
    }

    if participants.is_empty() {
        return PlantUmlNativeRender {
            original: source.to_string(),
            output: None,
            unsupported,
        };
    }

    let (bytes, width_px, height_px) = render_timing_svg(
        title.as_deref(),
        &mut participants,
        &messages,
        &constraints,
        &highlights,
        &mut tick_labels,
    );

    PlantUmlNativeRender {
        original: source.to_string(),
        output: Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }),
        unsupported,
    }
}

fn render_timing_svg(
    title: Option<&str>,
    participants: &mut [TimingParticipant],
    messages: &[TimingMessage],
    constraints: &[TimingConstraint],
    highlights: &[TimingHighlight],
    tick_labels: &mut Vec<(f64, String)>,
) -> (Vec<u8>, u32, u32) {
    let mut min_t = f64::INFINITY;
    let mut max_t = f64::NEG_INFINITY;
    for p in participants.iter_mut() {
        p.changes.sort_by(|a, b| a.0.total_cmp(&b.0));
        for &(t, _) in &p.changes {
            min_t = min_t.min(t);
            max_t = max_t.max(t);
        }
    }
    for m in messages {
        min_t = min_t.min(m.from_time).min(m.to_time);
        max_t = max_t.max(m.from_time).max(m.to_time);
    }
    for c in constraints {
        min_t = min_t.min(c.t1).min(c.t2);
        max_t = max_t.max(c.t1).max(c.t2);
    }
    for h in highlights {
        min_t = min_t.min(h.t1).min(h.t2);
        max_t = max_t.max(h.t1).max(h.t2);
    }
    for &(t, _) in tick_labels.iter() {
        min_t = min_t.min(t);
        max_t = max_t.max(t);
    }
    if !min_t.is_finite() {
        min_t = 0.0;
        max_t = 100.0;
    }
    let span = (max_t - min_t).max(1.0);
    // Add a trailing segment so the final state has visible width.
    let end_t = max_t + (span * 0.2).max(10.0);
    let total_span = (end_t - min_t).max(1.0);

    tick_labels.sort_by(|a, b| a.0.total_cmp(&b.0));
    if tick_labels.is_empty() {
        tick_labels.push((min_t, format_time_tick(min_t)));
        tick_labels.push((max_t, format_time_tick(max_t)));
    }

    let max_label_w = participants
        .iter()
        .flat_map(|p| {
            std::iter::once(estimate_text_width(&p.label, 12.5))
                .chain(p.states.iter().map(|s| estimate_text_width(s, 11.0)))
        })
        .fold(96.0f32, f32::max);

    let pad_x = 20.0f32;
    let label_col_w = (max_label_w + 28.0).clamp(110.0, 240.0);
    let plot_x = pad_x + label_col_w;
    let plot_w = ((tick_labels.len() as f32) * 90.0).clamp(440.0, 760.0);
    let width = (plot_x + plot_w + pad_x).ceil();

    let title_h = if title.is_some() { 34.0f32 } else { 14.0f32 };
    let lane_gap = 16.0f32;
    let mut lane_tops = Vec::with_capacity(participants.len());
    let mut lane_heights = Vec::with_capacity(participants.len());
    let mut y = title_h + 12.0;
    for p in participants.iter() {
        let h = match p.kind {
            TimingKind::Robust => ((p.states.len().max(2) as f32) * 26.0 + 36.0).max(88.0),
            TimingKind::Concise => 62.0,
            TimingKind::Binary | TimingKind::Clock => 56.0,
            TimingKind::Analog => 76.0,
        };
        lane_tops.push(y);
        lane_heights.push(h);
        y += h + lane_gap;
    }
    let axis_y = y + 6.0;
    let height = (axis_y + 34.0).ceil();

    let x_of = |t: f64| -> f32 { plot_x + (((t - min_t) / total_span) as f32) * plot_w };

    let mut svg = String::with_capacity(4096);
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0} {height:.0}" width="{width:.0}" height="{height:.0}" font-family="system-ui, -apple-system, 'Segoe UI', sans-serif">"##
    ));
    svg.push_str(
        r##"<defs><marker id="timing-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 1 L 10 5 L 0 9 z" fill="#2563eb"/></marker></defs>"##,
    );
    svg.push_str(&format!(
        r##"<rect x="0" y="0" width="{width:.0}" height="{height:.0}" fill="#ffffff"/>"##
    ));

    if let Some(t) = title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="24" text-anchor="middle" font-size="15" font-weight="600" fill="#0f172a">{}</text>"##,
            width / 2.0,
            svg_escape(t)
        ));
    }

    // Highlight bands behind lanes.
    let bands_top = title_h + 6.0;
    let bands_h = (axis_y - bands_top).max(10.0);
    for h in highlights {
        let x1 = x_of(h.t1);
        let x2 = x_of(h.t2).max(x1 + 4.0);
        svg.push_str(&format!(
            r##"<rect x="{x1:.1}" y="{bands_top:.1}" width="{:.1}" height="{bands_h:.1}" fill="{}" opacity="0.45" data-shape="timing-highlight"/>"##,
            x2 - x1,
            svg_escape(&h.color)
        ));
        if let Some(lbl) = &h.label {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#92400e">{}</text>"##,
                (x1 + x2) / 2.0,
                bands_top + 12.0,
                svg_escape(lbl)
            ));
        }
    }

    // Vertical grid lines for ticks.
    for &(t, ref lbl) in tick_labels.iter() {
        let x = x_of(t);
        svg.push_str(&format!(
            r##"<line x1="{x:.1}" y1="{bands_top:.1}" x2="{x:.1}" y2="{axis_y:.1}" stroke="#e2e8f0" stroke-width="1" stroke-dasharray="3,3"/>"##
        ));
        svg.push_str(&format!(
            r##"<line x1="{x:.1}" y1="{axis_y:.1}" x2="{x:.1}" y2="{:.1}" stroke="#64748b" stroke-width="1.2"/>"##,
            axis_y + 5.0
        ));
        svg.push_str(&format!(
            r##"<text x="{x:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#475569">{}</text>"##,
            axis_y + 20.0,
            svg_escape(lbl)
        ));
    }

    // Draw each participant lane.
    for (i, p) in participants.iter().enumerate() {
        let top = lane_tops[i];
        let h = lane_heights[i];
        let bot = top + h;
        svg.push_str(&format!(
            r##"<rect x="{pad_x:.1}" y="{top:.1}" width="{:.1}" height="{h:.1}" rx="6" fill="#f8fafc" stroke="#cbd5e1" stroke-width="1"/>"##,
            width - 2.0 * pad_x
        ));
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" font-size="12.5" font-weight="600" fill="#0f172a">{}</text>"##,
            pad_x + 10.0,
            top + 18.0,
            svg_escape(&p.label)
        ));

        match p.kind {
            TimingKind::Robust => {
                let n_states = p.states.len().max(1);
                let inner_top = top + 28.0;
                let inner_h = (h - 38.0).max(20.0);
                let y_for_state = |st: &str| -> f32 {
                    let pos = p.states.iter().position(|s| s == st).unwrap_or(0);
                    if n_states <= 1 {
                        inner_top + inner_h / 2.0
                    } else {
                        // First state at bottom, later states stepped upward.
                        inner_top + inner_h * (1.0 - (pos as f32) / ((n_states - 1) as f32))
                    }
                };
                for st in &p.states {
                    let sy = y_for_state(st);
                    svg.push_str(&format!(
                        r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10.5" fill="#475569">{}</text>"##,
                        plot_x - 8.0,
                        sy + 3.5,
                        svg_escape(st)
                    ));
                    svg.push_str(&format!(
                        r##"<line x1="{plot_x:.1}" y1="{sy:.1}" x2="{:.1}" y2="{sy:.1}" stroke="#e2e8f0" stroke-width="0.8"/>"##,
                        plot_x + plot_w
                    ));
                }
                let mut d = String::new();
                let mut prev_y: Option<f32> = None;
                for (ci, (t, st)) in p.changes.iter().enumerate() {
                    if matches!(st.as_str(), "{hidden}" | "{-}") {
                        prev_y = None;
                        continue;
                    }
                    let x1 = x_of(*t);
                    let x2 = p
                        .changes
                        .get(ci + 1)
                        .map_or(plot_x + plot_w, |(nt, _)| x_of(*nt));
                    let sy = y_for_state(st);
                    if let Some(py) = prev_y {
                        d.push_str(&format!(" L {x1:.1} {py:.1} L {x1:.1} {sy:.1}"));
                    } else {
                        d.push_str(&format!(" M {x1:.1} {sy:.1}"));
                    }
                    d.push_str(&format!(" L {x2:.1} {sy:.1}"));
                    prev_y = Some(sy);
                }
                if !d.is_empty() {
                    svg.push_str(&format!(
                        r##"<path d="{}" fill="none" stroke="#1d4ed8" stroke-width="2" data-shape="timing-robust"/>"##,
                        d.trim()
                    ));
                }
            }
            TimingKind::Concise => {
                let cy = top + h * 0.62;
                let hh = 13.0f32;
                for (ci, (t, st)) in p.changes.iter().enumerate() {
                    if matches!(st.as_str(), "{hidden}" | "{-}") {
                        continue;
                    }
                    let x1 = x_of(*t);
                    let x2 = p
                        .changes
                        .get(ci + 1)
                        .map_or(plot_x + plot_w, |(nt, _)| x_of(*nt));
                    let w = (x2 - x1).max(12.0);
                    let bevel = (w * 0.15).min(8.0);
                    let pts = format!(
                        "{x1:.1},{cy:.1} {:.1},{:.1} {:.1},{:.1} {x2:.1},{cy:.1} {:.1},{:.1} {:.1},{:.1}",
                        x1 + bevel,
                        cy - hh,
                        x2 - bevel,
                        cy - hh,
                        x2 - bevel,
                        cy + hh,
                        x1 + bevel,
                        cy + hh
                    );
                    svg.push_str(&format!(
                        r##"<polygon points="{pts}" fill="#dbeafe" stroke="#2563eb" stroke-width="1.4" data-shape="timing-concise"/>"##
                    ));
                    svg.push_str(&format!(
                        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#1e3a8a">{}</text>"##,
                        (x1 + x2) / 2.0,
                        cy + 4.0,
                        svg_escape(st)
                    ));
                }
            }
            TimingKind::Binary => {
                let y_hi = top + 24.0;
                let y_lo = bot - 10.0;
                let mut d = String::new();
                let mut prev_y: Option<f32> = None;
                for (ci, (t, st)) in p.changes.iter().enumerate() {
                    let is_hi = matches!(
                        st.to_ascii_lowercase().as_str(),
                        "high" | "1" | "true" | "on"
                    );
                    let sy = if is_hi { y_hi } else { y_lo };
                    let x1 = x_of(*t);
                    let x2 = p
                        .changes
                        .get(ci + 1)
                        .map_or(plot_x + plot_w, |(nt, _)| x_of(*nt));
                    if let Some(py) = prev_y {
                        d.push_str(&format!(" L {x1:.1} {py:.1} L {x1:.1} {sy:.1}"));
                    } else {
                        d.push_str(&format!(" M {x1:.1} {sy:.1}"));
                    }
                    d.push_str(&format!(" L {x2:.1} {sy:.1}"));
                    prev_y = Some(sy);
                }
                if !d.is_empty() {
                    svg.push_str(&format!(
                        r##"<path d="{}" fill="none" stroke="#0f766e" stroke-width="2" data-shape="timing-binary"/>"##,
                        d.trim()
                    ));
                }
            }
            TimingKind::Clock => {
                let period = p.clock_period.unwrap_or_else(|| (total_span / 8.0).max(1.0));
                let half = (period / 2.0).max(0.5);
                let y_hi = top + 24.0;
                let y_lo = bot - 10.0;
                let mut d = String::new();
                let mut t = min_t;
                let mut hi = true;
                let mut steps = 0usize;
                while t < end_t && steps < 200 {
                    let nt = (t + half).min(end_t);
                    let x1 = x_of(t);
                    let x2 = x_of(nt);
                    let sy = if hi { y_hi } else { y_lo };
                    if steps == 0 {
                        d.push_str(&format!("M {x1:.1} {sy:.1} L {x2:.1} {sy:.1}"));
                    } else {
                        d.push_str(&format!(" L {x1:.1} {sy:.1} L {x2:.1} {sy:.1}"));
                    }
                    t = nt;
                    hi = !hi;
                    steps += 1;
                }
                svg.push_str(&format!(
                    r##"<path d="{d}" fill="none" stroke="#7c3aed" stroke-width="1.8" data-shape="timing-clock"/>"##
                ));
            }
            TimingKind::Analog => {
                let vals: Vec<(f64, f64)> = p
                    .changes
                    .iter()
                    .filter_map(|(t, s)| s.parse::<f64>().ok().map(|v| (*t, v)))
                    .collect();
                if !vals.is_empty() {
                    let min_v = vals.iter().map(|v| v.1).fold(f64::INFINITY, f64::min);
                    let max_v = vals.iter().map(|v| v.1).fold(f64::NEG_INFINITY, f64::max);
                    let vspan = (max_v - min_v).max(1.0);
                    let inner_top = top + 26.0;
                    let inner_h = (h - 36.0).max(20.0);
                    let mut pts = Vec::with_capacity(vals.len());
                    for (t, v) in &vals {
                        let x = x_of(*t);
                        let py = inner_top + inner_h * (1.0 - ((*v - min_v) / vspan) as f32);
                        pts.push(format!("{x:.1},{py:.1}"));
                    }
                    svg.push_str(&format!(
                        r##"<polyline points="{}" fill="none" stroke="#b45309" stroke-width="2" data-shape="timing-analog"/>"##,
                        pts.join(" ")
                    ));
                }
            }
        }
    }

    // Cross-participant messages.
    for m in messages {
        let x1 = x_of(m.from_time);
        let y1 = lane_tops[m.from_idx] + lane_heights[m.from_idx] * 0.55;
        let x2 = x_of(m.to_time);
        let y2 = lane_tops[m.to_idx] + lane_heights[m.to_idx] * 0.55;
        svg.push_str(&format!(
            r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="#2563eb" stroke-width="1.5" marker-end="url(#timing-arrow)"/>"##
        ));
        if !m.label.is_empty() {
            let mx = (x1 + x2) / 2.0 + 6.0;
            let my = (y1 + y2) / 2.0 - 4.0;
            svg.push_str(&format!(
                r##"<text x="{mx:.1}" y="{my:.1}" font-size="11" fill="#1d4ed8">{}</text>"##,
                svg_escape(&m.label)
            ));
        }
    }

    // Duration constraints (`@100 <-> @300 : {200 ms}`).
    for c in constraints {
        let x1 = x_of(c.t1);
        let x2 = x_of(c.t2);
        let cy = lane_tops[c.participant_idx] + 14.0;
        svg.push_str(&format!(
            r##"<line x1="{x1:.1}" y1="{cy:.1}" x2="{x2:.1}" y2="{cy:.1}" stroke="#dc2626" stroke-width="1.2" stroke-dasharray="4,2"/>"##
        ));
        if !c.label.is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#dc2626">{}</text>"##,
                (x1 + x2) / 2.0,
                cy - 3.0,
                svg_escape(&c.label)
            ));
        }
    }

    // Bottom time axis.
    svg.push_str(&format!(
        r##"<line x1="{plot_x:.1}" y1="{axis_y:.1}" x2="{:.1}" y2="{axis_y:.1}" stroke="#475569" stroke-width="1.4"/>"##,
        plot_x + plot_w
    ));
    svg.push_str("</svg>");
    (svg.into_bytes(), width as u32, height as u32)
}

fn format_time_tick(t: f64) -> String {
    if (t.fract()).abs() < 1e-6 {
        format!("{}", t as i64)
    } else {
        format!("{t:.1}")
    }
}

fn parse_time_val(raw: &str, base: f64) -> Option<f64> {
    let s = raw.trim().trim_start_matches('@');
    if let Some(rel) = s.strip_prefix('+') {
        return rel.trim().parse::<f64>().ok().map(|d| base + d);
    }
    if let Some(rel) = s.strip_prefix('-') {
        return rel.trim().parse::<f64>().ok().map(|d| base - d);
    }
    s.parse::<f64>().ok()
}

fn resolve_time_ref(raw: &str, base: f64, named: &HashMap<String, f64>) -> Option<f64> {
    let s = raw.trim().trim_start_matches('@');
    if let Some(&t) = named.get(s) {
        return Some(t);
    }
    parse_time_val(s, base)
}

fn split_target_time(s: &str) -> (&str, Option<&str>) {
    match s.split_once('@') {
        Some((name, t)) => (name.trim(), Some(t.trim())),
        None => (s.trim(), None),
    }
}

fn split_timing_message(line: &str) -> Option<(&str, &str, Option<&str>)> {
    let (expr, lbl) = match split_colon_outside_quotes(line) {
        Some((e, l)) => (e.trim(), Some(l.trim())),
        None => (line.trim(), None),
    };
    if let Some((a, b)) = expr.split_once("->") {
        let a = a.trim();
        let b = b.trim();
        if !a.is_empty() && !b.is_empty() {
            return Some((a, b, lbl));
        }
    }
    None
}

fn parse_constraint_line(
    line: &str,
    participant_idx: usize,
    current_time: f64,
    named: &HashMap<String, f64>,
) -> Option<TimingConstraint> {
    let (expr, lbl) = match split_colon_outside_quotes(line) {
        Some((e, l)) => (e.trim(), sanitize_label(l.trim().trim_matches(['{', '}']))),
        None => (line.trim(), String::new()),
    };
    let (a, b) = expr.split_once("<->")?;
    let t1 = resolve_time_ref(a.trim(), current_time, named)?;
    let t2 = resolve_time_ref(b.trim(), t1, named)?;
    Some(TimingConstraint {
        participant_idx,
        t1: t1.min(t2),
        t2: t1.max(t2),
        label: lbl,
    })
}

fn split_word_ci<'a>(s: &'a str, word: &str) -> Option<(&'a str, &'a str)> {
    let mut in_quotes = false;
    let bytes = s.as_bytes();
    let wlen = word.len();
    for i in 0..bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
            continue;
        }
        if !in_quotes
            && i > 0
            && bytes[i - 1].is_ascii_whitespace()
            && i + wlen < bytes.len()
            && bytes[i + wlen].is_ascii_whitespace()
            && s.is_char_boundary(i)
            && s.is_char_boundary(i + wlen)
            && s[i..i + wlen].eq_ignore_ascii_case(word)
        {
            return Some((s[..i].trim(), s[i + wlen..].trim()));
        }
    }
    None
}

fn extract_color_token(s: &str) -> Option<String> {
    for tok in s.split_whitespace() {
        if let Some(hex) = tok.strip_prefix('#')
            && !hex.is_empty()
            && hex.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Some(format!("#{hex}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::mermaid::{MermaidOutput, render_plantuml};

    #[test]
    fn renders_plantuml_timing_diagram_svg() {
        let puml = "\
@startuml
title 通信タイミング図
robust \"Web Browser\" as WB
concise \"Web User\" as WU
binary \"Enable\" as EN
clock \"Clk\" as C with period 50

@0
WU is Idle
WB is Idle
EN is low

@100
WU -> WB : URL
WU is Waiting
WB is Processing
EN is high

@250
WB -> WU@+50 : OK
WB is Idle
EN is low
WB@100 <-> @250 : {150 ms}
highlight 100 to 250 #Gold : 処理中

@300
WU is Done
@enduml
";
        let res = render_plantuml(puml);
        assert!(res.is_lossless(), "unexpected diagnostics: {:?}", res.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res.output else {
            panic!("expected SVG for PlantUML timing diagram");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        assert!(svg.contains("通信タイミング図"), "{svg}");
        assert!(svg.contains("Web Browser") && svg.contains("Web User"), "{svg}");
        assert!(svg.contains(r#"data-shape="timing-robust""#), "{svg}");
        assert!(svg.contains(r#"data-shape="timing-concise""#), "{svg}");
        assert!(svg.contains(r#"data-shape="timing-binary""#), "{svg}");
        assert!(svg.contains(r#"data-shape="timing-clock""#), "{svg}");
        assert!(svg.contains("150 ms") && svg.contains("URL"), "{svg}");
    }
}
