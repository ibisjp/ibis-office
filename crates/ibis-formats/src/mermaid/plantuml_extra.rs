// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Structural and planning PlantUML diagram converters (Mindmap, WBS, Gantt,
//! Activity, Component/Deployment) sharing the existing Mermaid `mindmap`,
//! `treeview`, `gantt`, and `flowchart` renderers without duplicating rendering code.

use std::collections::HashMap;

use super::plantuml::{
    PlantUmlUnsupported, excerpt, sanitize_label, split_as_outside_quotes,
    split_colon_outside_quotes, strip_block_comments, strip_prefix_ci, strip_quotes,
};
use super::plantuml_uml::{
    UmlSkip, advance_skip, handle_common_ignored_or_unsupported, normalize_directional_arrows,
    parse_uml_entity_head, prepend_frontmatter_title, safe_id, strip_stereotype,
};

use super::plantuml_style::{
    normalize_creole_inline, normalize_plantuml_color, split_and_normalize_color,
};

fn escape_flow_label(s: &str) -> String {
    normalize_creole_inline(&sanitize_label(s)).replace('"', "'")
}

// ============================================================================
// 1. PlantUML Mindmap (`@startmindmap`) -> Mermaid `mindmap`
// ============================================================================

pub(super) fn convert_plantuml_mindmap(
    source: &str,
) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut lines_out: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_end = false;

    for (idx, raw) in source.lines().enumerate() {
        let line_no = idx + 1;
        if saw_end {
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
            saw_end = true;
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
        if lower == "left side" || lower == "right side" {
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "title ") {
            let t = sanitize_label(rest);
            if !t.is_empty() {
                title = Some(t);
            }
            continue;
        }
        if let Some((depth, label, had_color)) = parse_outline_item(trimmed) {
            if had_color {
                unsupported.push(PlantUmlUnsupported {
                    line: line_no,
                    kind: "styling",
                    source: excerpt(trimmed, 120),
                    reason: "Mindmap node color is not preserved.".to_string(),
                });
            }
            let indent = "  ".repeat(depth);
            let clean = sanitize_label(label);
            if !clean.is_empty() {
                lines_out.push(format!("{indent}{clean}"));
            }
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported PlantUML mindmap syntax.".to_string(),
        });
    }

    if lines_out.is_empty() {
        return (None, unsupported);
    }
    (
        Some(prepend_frontmatter_title(
            "mindmap",
            title.as_deref(),
            &lines_out,
        )),
        unsupported,
    )
}

/// Parse PlantUML mindmap/WBS outline item (`* Root`, `** Child`, `**_ Boxless`, `*[#Red] Colored`).
fn parse_outline_item(line: &str) -> Option<(usize, &str, bool)> {
    let first = line.as_bytes().first().copied()?;
    if !matches!(first, b'*' | b'+' | b'-') {
        return None;
    }
    let depth = line.bytes().take_while(|&b| b == first).count();
    let mut rest = &line[depth..];
    // Optional PlantUML modifiers: `_` (boxless), `<` / `>` (WBS direction).
    rest = rest.trim_start_matches(['_', '<', '>']);
    let mut had_color = false;
    if let Some(after_bracket) = rest.strip_prefix("[#")
        && let Some(close) = after_bracket.find(']')
    {
        had_color = true;
        rest = &after_bracket[close + 1..];
    }
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let text = rest.trim();
    (!text.is_empty()).then_some((depth, text, had_color))
}

// ============================================================================
// 2. PlantUML WBS (`@startwbs`) -> Mermaid `treeView-beta`
// ============================================================================

pub(super) fn convert_plantuml_wbs(source: &str) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut lines_out: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_end = false;

    for (idx, raw) in source.lines().enumerate() {
        let line_no = idx + 1;
        if saw_end {
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
            saw_end = true;
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
        if let Some((depth, label, had_color)) = parse_outline_item(trimmed) {
            if had_color {
                unsupported.push(PlantUmlUnsupported {
                    line: line_no,
                    kind: "styling",
                    source: excerpt(trimmed, 120),
                    reason: "WBS node color is not preserved.".to_string(),
                });
            }
            let indent = "    ".repeat(depth);
            let clean = sanitize_label(label).replace('"', "'");
            lines_out.push(format!("{indent}\"{clean}\""));
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported PlantUML WBS syntax.".to_string(),
        });
    }

    if lines_out.is_empty() {
        return (None, unsupported);
    }
    (
        Some(prepend_frontmatter_title(
            "treeView-beta",
            title.as_deref(),
            &lines_out,
        )),
        unsupported,
    )
}

// ============================================================================
// 3. PlantUML Gantt (`@startgantt`) -> Mermaid `gantt`
// ============================================================================

struct GanttTaskRec {
    id: String,
    name: String,
    section: Option<String>,
    start: Option<String>,
    after: Vec<String>,
    duration: String,
    milestone: bool,
    done: bool,
}

pub(super) fn convert_plantuml_gantt(source: &str) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut title: Option<String> = None;
    let mut project_start = "2026-01-01".to_string();
    let mut excludes: Vec<&'static str> = Vec::new();
    let mut tasks: Vec<GanttTaskRec> = Vec::new();
    let mut task_map: HashMap<String, usize> = HashMap::new();
    let mut current_section: Option<String> = None;
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_end = false;

    let ensure_task = |name: &str,
                       section: &Option<String>,
                       tasks: &mut Vec<GanttTaskRec>,
                       task_map: &mut HashMap<String, usize>|
     -> usize {
        if let Some(&idx) = task_map.get(name) {
            return idx;
        }
        let idx = tasks.len();
        let id = format!("t{}", idx + 1);
        tasks.push(GanttTaskRec {
            id,
            name: sanitize_label(name),
            section: section.clone(),
            start: None,
            after: Vec::new(),
            duration: "1d".to_string(),
            milestone: false,
            done: false,
        });
        task_map.insert(name.to_string(), idx);
        idx
    };

    for (idx, raw) in source.lines().enumerate() {
        let line_no = idx + 1;
        if saw_end {
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
            saw_end = true;
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
        // `Project starts 2026-04-01` / `Project starts the 2026-04-01`
        if let Some(rest) = strip_prefix_ci(trimmed, "project starts") {
            if let Some(date) = extract_iso_date(rest) {
                project_start = date.to_string();
            }
            continue;
        }
        if lower.contains("saturday") && lower.contains("closed") {
            if !excludes.contains(&"saturday") {
                excludes.push("saturday");
            }
            continue;
        }
        if lower.contains("sunday") && lower.contains("closed") {
            if !excludes.contains(&"sunday") {
                excludes.push("sunday");
            }
            continue;
        }
        // Section divider `-- Phase 1 --`
        if trimmed.starts_with("--") && trimmed.ends_with("--") && trimmed.len() >= 4 {
            let sec = sanitize_label(trimmed.trim_matches('-').trim());
            current_section = (!sec.is_empty()).then_some(sec);
            continue;
        }
        // Dependency arrow `[A] -> [B]`
        if let Some((lhs_raw, rhs_raw)) = trimmed.split_once("->")
            && let Some(lhs) = extract_bracket_name(lhs_raw.trim())
            && let Some(rhs) = extract_bracket_name(rhs_raw.trim())
        {
            let from = ensure_task(lhs, &current_section, &mut tasks, &mut task_map);
            let to = ensure_task(rhs, &current_section, &mut tasks, &mut task_map);
            let from_id = tasks[from].id.clone();
            if !tasks[to].after.contains(&from_id) {
                tasks[to].after.push(from_id);
            }
            continue;
        }
        // Task statement starting with `[Task Name]`
        if let Some((name, rest)) = split_leading_bracket_name(trimmed) {
            let mut rest = rest.trim();
            // Optional alias `as [T1]`
            if let Some(after_as) = strip_prefix_ci(rest, "as ")
                && let Some((alias, tail)) = split_leading_bracket_name(after_as.trim())
            {
                let tidx = ensure_task(name, &current_section, &mut tasks, &mut task_map);
                task_map.insert(alias.to_string(), tidx);
                rest = tail.trim();
            }
            let tidx = ensure_task(name, &current_section, &mut tasks, &mut task_map);
            if rest.is_empty() {
                continue;
            }
            let rest_lower = rest.to_ascii_lowercase();
            if rest_lower.contains("is colored") {
                unsupported.push(PlantUmlUnsupported {
                    line: line_no,
                    kind: "styling",
                    source: excerpt(trimmed, 120),
                    reason: "Task color styling is not preserved in Mermaid gantt.".to_string(),
                });
                continue;
            }
            // Parse clauses (`lasts N days/weeks`, `starts YYYY-MM-DD`, `starts at [X]'s end`,
            // `happens at ...`, `is 100% completed`, `and ...`).
            let mut handled_any = false;
            for clause in split_gantt_clauses(rest) {
                let c = clause.trim();
                let cl = c.to_ascii_lowercase();
                if let Some(dur) = parse_gantt_lasts(c) {
                    tasks[tidx].duration = dur;
                    handled_any = true;
                } else if let Some(after_h) = strip_prefix_ci(c, "happens") {
                    tasks[tidx].milestone = true;
                    tasks[tidx].duration = "0d".to_string();
                    if let Some(date) = extract_iso_date(after_h) {
                        tasks[tidx].start = Some(date.to_string());
                    } else if let Some(dep_name) = extract_bracket_name(after_h) {
                        let dep_idx =
                            ensure_task(dep_name, &current_section, &mut tasks, &mut task_map);
                        let dep_id = tasks[dep_idx].id.clone();
                        if !tasks[tidx].after.contains(&dep_id) {
                            tasks[tidx].after.push(dep_id);
                        }
                    }
                    handled_any = true;
                } else if let Some(after_s) = strip_prefix_ci(c, "starts") {
                    if let Some(date) = extract_iso_date(after_s) {
                        tasks[tidx].start = Some(date.to_string());
                    } else if let Some(dep_name) = extract_bracket_name(after_s) {
                        let dep_idx =
                            ensure_task(dep_name, &current_section, &mut tasks, &mut task_map);
                        let dep_id = tasks[dep_idx].id.clone();
                        if !tasks[tidx].after.contains(&dep_id) {
                            tasks[tidx].after.push(dep_id);
                        }
                    }
                    handled_any = true;
                } else if cl.contains("100%") && cl.contains("completed") {
                    tasks[tidx].done = true;
                    handled_any = true;
                } else if let Some(after_req) = strip_prefix_ci(c, "requires") {
                    if let Some(dep_name) = extract_bracket_name(after_req) {
                        let dep_idx =
                            ensure_task(dep_name, &current_section, &mut tasks, &mut task_map);
                        let dep_id = tasks[dep_idx].id.clone();
                        if !tasks[tidx].after.contains(&dep_id) {
                            tasks[tidx].after.push(dep_id);
                        }
                        handled_any = true;
                    }
                }
            }
            if !handled_any {
                unsupported.push(PlantUmlUnsupported {
                    line: line_no,
                    kind: "syntax",
                    source: excerpt(trimmed, 120),
                    reason: "Unsupported PlantUML gantt task clause.".to_string(),
                });
            }
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported PlantUML gantt syntax.".to_string(),
        });
    }

    if tasks.is_empty() {
        return (None, unsupported);
    }
    let mut lines_out = Vec::new();
    if let Some(t) = title {
        lines_out.push(format!("  title {t}"));
    }
    lines_out.push("  dateFormat YYYY-MM-DD".to_string());
    if excludes.contains(&"saturday") && excludes.contains(&"sunday") {
        lines_out.push("  excludes weekends".to_string());
    } else if !excludes.is_empty() {
        lines_out.push(format!("  excludes {}", excludes.join(", ")));
    }
    let mut last_section: Option<String> = None;
    for task in &tasks {
        if task.section != last_section {
            if let Some(sec) = &task.section {
                lines_out.push(format!("  section {sec}"));
            }
            last_section = task.section.clone();
        }
        let mut meta = Vec::new();
        if task.milestone {
            meta.push("milestone".to_string());
        } else if task.done {
            meta.push("done".to_string());
        }
        meta.push(task.id.clone());
        if !task.after.is_empty() {
            meta.push(format!("after {}", task.after.join(" ")));
        } else if let Some(start) = &task.start {
            meta.push(start.clone());
        } else {
            meta.push(project_start.clone());
        }
        meta.push(task.duration.clone());
        let name = task.name.replace(':', " -");
        lines_out.push(format!("  {name} :{}", meta.join(", ")));
    }

    let mut out = String::from("gantt\n");
    for line in lines_out {
        out.push_str(&line);
        out.push('\n');
    }
    (Some(out), unsupported)
}

fn split_leading_bracket_name(s: &str) -> Option<(&str, &str)> {
    let rest = s.strip_prefix('[')?;
    let close = rest.find(']')?;
    let name = rest[..close].trim();
    (!name.is_empty()).then_some((name, &rest[close + 1..]))
}

fn extract_bracket_name(s: &str) -> Option<&str> {
    let open = s.find('[')?;
    let rest = &s[open + 1..];
    let close = rest.find(']')?;
    let name = rest[..close].trim();
    (!name.is_empty()).then_some(name)
}

fn extract_iso_date(s: &str) -> Option<&str> {
    for tok in s.split_whitespace() {
        let tok = tok.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
        if tok.len() == 10
            && tok.as_bytes()[4] == b'-'
            && tok.as_bytes()[7] == b'-'
            && tok
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
        {
            return Some(tok);
        }
    }
    None
}

fn split_gantt_clauses(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut in_bracket = false;
    for i in 0..bytes.len() {
        match bytes[i] {
            b'[' => in_bracket = true,
            b']' => in_bracket = false,
            _ => {}
        }
        if !in_bracket
            && i + 5 <= bytes.len()
            && s.is_char_boundary(i)
            && s.is_char_boundary(i + 5)
            && s[i..i + 5].eq_ignore_ascii_case(" and ")
        {
            parts.push(s[start..i].trim());
            start = i + 5;
        }
    }
    parts.push(s[start..].trim());
    parts
}

fn parse_gantt_lasts(clause: &str) -> Option<String> {
    let rest = strip_prefix_ci(clause.trim(), "lasts ")?.trim();
    let mut words = rest.split_whitespace();
    let n: usize = words.next()?.parse().ok()?;
    let unit = words.next().unwrap_or("days").to_ascii_lowercase();
    let suffix = if unit.starts_with('w') {
        "w"
    } else if unit.starts_with('h') {
        "h"
    } else {
        "d"
    };
    Some(format!("{n}{suffix}"))
}

// ============================================================================
// 4. PlantUML Activity Diagram (new syntax) -> Mermaid `flowchart TD`
// ============================================================================

#[derive(Clone)]
struct FlowTail {
    node: String,
    edge_label: Option<String>,
}

enum ActFrame {
    If {
        cond_node: String,
        false_label: Option<String>,
        exits: Vec<FlowTail>,
        in_else: bool,
    },
    Switch {
        cond_node: String,
        exits: Vec<FlowTail>,
        in_case: bool,
    },
    While {
        cond_node: String,
    },
    Repeat {
        entry_node: String,
    },
    Fork {
        fork_node: String,
        branch_exits: Vec<FlowTail>,
    },
    Split {
        entry_tails: Vec<FlowTail>,
        branch_exits: Vec<FlowTail>,
    },
}

pub(super) fn convert_plantuml_activity(
    source: &str,
) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    // Detect legacy activity syntax (`(*) --> ...` without new-syntax `start`/`:...;`).
    if is_legacy_activity(source) {
        return convert_plantuml_activity_legacy(source);
    }

    let mut unsupported = Vec::new();
    let mut lines_out: Vec<String> = Vec::new();
    let mut edge_lines: Vec<String> = Vec::new();
    let mut lanes: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut lane_map: HashMap<String, usize> = HashMap::new();
    let mut current_lane: Option<usize> = None;
    let mut pre_lane_nodes: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_enduml = false;
    let mut next_id: usize = 1;
    let mut tails: Vec<FlowTail> = Vec::new();
    let mut frames: Vec<ActFrame> = Vec::new();
    let mut open_partitions: usize = 0;
    let mut pending_action: Option<Vec<String>> = None;
    let mut pending_note = false;
    let mut has_content = false;

    let mut fresh = |prefix: &str| -> String {
        let id = format!("{prefix}{next_id}");
        next_id += 1;
        id
    };

    let push_node = |decl: String,
                     current_lane: Option<usize>,
                     lanes: &mut [(String, String, Vec<String>)],
                     pre_lane_nodes: &mut Vec<String>,
                     lines_out: &mut Vec<String>| {
        lines_out.push(format!("  {decl}"));
        if let Some(li) = current_lane {
            lanes[li].2.push(format!("    {decl}"));
        } else {
            pre_lane_nodes.push(format!("    {decl}"));
        }
    };

    let connect_to = |target: &str,
                      tails: &mut Vec<FlowTail>,
                      lines: &mut Vec<String>,
                      edges: &mut Vec<String>| {
        for tail in tails.drain(..) {
            let line = match tail.edge_label.filter(|l| !l.is_empty()) {
                Some(lbl) => {
                    let clean = lbl.replace('|', "/");
                    format!("  {} -->|\"{clean}\"| {target}", tail.node)
                }
                None => format!("  {} --> {target}", tail.node),
            };
            lines.push(line.clone());
            edges.push(line);
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
        if pending_note {
            let lower = trimmed.to_ascii_lowercase();
            if lower == "end note" || lower == "endnote" {
                pending_note = false;
            }
            continue;
        }
        if let Some(mut act_lines) = pending_action.take() {
            if let Some(end_part) = trimmed.strip_suffix(';') {
                if !end_part.trim().is_empty() {
                    act_lines.push(end_part.trim().to_string());
                }
                let lbl = escape_flow_label(&act_lines.join(" / "));
                let id = fresh("A");
                push_node(
                    format!("{id}(\"{lbl}\")"),
                    current_lane,
                    &mut lanes,
                    &mut pre_lane_nodes,
                    &mut lines_out,
                );
                connect_to(&id, &mut tails, &mut lines_out, &mut edge_lines);
                tails.push(FlowTail {
                    node: id,
                    edge_label: None,
                });
                has_content = true;
            } else {
                if !trimmed.is_empty() {
                    act_lines.push(trimmed.to_string());
                }
                pending_action = Some(act_lines);
            }
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
        // Swimlane switch: `|Lane Name|` or `|#AntiqueWhite|Lane Name|`
        if let Some(lane_title) = parse_swimlane_header(trimmed) {
            let lbl = escape_flow_label(lane_title);
            let li = if let Some(&existing) = lane_map.get(&lbl) {
                existing
            } else {
                let idx = lanes.len();
                let lid = format!("Lane{}", idx + 1);
                let mut init_nodes = Vec::new();
                if idx == 0 && !pre_lane_nodes.is_empty() {
                    init_nodes.append(&mut pre_lane_nodes);
                }
                lanes.push((lid, lbl.clone(), init_nodes));
                lane_map.insert(lbl, idx);
                idx
            };
            current_lane = Some(li);
            continue;
        }
        if lower == "start" {
            let id = fresh("S");
            push_node(
                format!("{id}@{{ shape: start }}"),
                current_lane,
                &mut lanes,
                &mut pre_lane_nodes,
                &mut lines_out,
            );
            connect_to(&id, &mut tails, &mut lines_out, &mut edge_lines);
            tails.push(FlowTail {
                node: id,
                edge_label: None,
            });
            has_content = true;
            continue;
        }
        if lower == "stop" || lower == "end" || lower == "detach" || lower == "kill" {
            let id = fresh("E");
            push_node(
                format!("{id}@{{ shape: stop }}"),
                current_lane,
                &mut lanes,
                &mut pre_lane_nodes,
                &mut lines_out,
            );
            connect_to(&id, &mut tails, &mut lines_out, &mut edge_lines);
            has_content = true;
            continue;
        }
        // `partition "Name" {` / `}`
        if let Some(rest) = strip_prefix_ci(trimmed, "partition ") {
            let opens = rest.trim_end().ends_with('{');
            let head = rest.trim_end().trim_end_matches('{').trim();
            let (id, label) = parse_uml_entity_head(head);
            let lbl = escape_flow_label(label.as_deref().unwrap_or(&id));
            if opens {
                lines_out.push(format!("  subgraph {id}[\"{lbl}\"]"));
                open_partitions += 1;
            }
            continue;
        }
        if trimmed == "}" && open_partitions > 0 {
            open_partitions -= 1;
            lines_out.push("  end".to_string());
            continue;
        }
        // Action `:text;` or colored `#Color:text;` (single-line or multi-line)
        if let Some((action_color, after_colon)) = split_activity_action_prefix(trimmed) {
            let fill_hex = action_color.and_then(normalize_plantuml_color);
            if let Some(inner) = after_colon.strip_suffix(';') {
                let lbl = escape_flow_label(inner);
                let id = fresh("A");
                push_node(
                    format!("{id}(\"{lbl}\")"),
                    current_lane,
                    &mut lanes,
                    &mut pre_lane_nodes,
                    &mut lines_out,
                );
                if let Some(hex) = fill_hex {
                    let st = format!("  style {id} fill:{hex}");
                    lines_out.push(st.clone());
                    edge_lines.push(st);
                }
                connect_to(&id, &mut tails, &mut lines_out, &mut edge_lines);
                tails.push(FlowTail {
                    node: id,
                    edge_label: None,
                });
                has_content = true;
            } else {
                pending_action = Some(vec![after_colon.trim().to_string()]);
            }
            continue;
        }
        // `if (cond) then (yes)`
        if let Some((cond, yes_lbl)) = parse_act_if(trimmed, "if") {
            let d = fresh("D");
            push_node(
                format!("{d}{{\"{}\"}}", escape_flow_label(cond)),
                current_lane,
                &mut lanes,
                &mut pre_lane_nodes,
                &mut lines_out,
            );
            connect_to(&d, &mut tails, &mut lines_out, &mut edge_lines);
            frames.push(ActFrame::If {
                cond_node: d.clone(),
                false_label: None,
                exits: Vec::new(),
                in_else: false,
            });
            tails.push(FlowTail {
                node: d,
                edge_label: yes_lbl.map(escape_flow_label),
            });
            has_content = true;
            continue;
        }
        // `elseif (cond) then (yes)`
        if let Some((cond, yes_lbl)) = parse_act_if(trimmed, "elseif") {
            if let Some(ActFrame::If {
                cond_node,
                exits,
                ..
            }) = frames.last_mut()
            {
                exits.extend(tails.drain(..));
                let next_d = fresh("D");
                push_node(
                    format!("{next_d}{{\"{}\"}}", escape_flow_label(cond)),
                    current_lane,
                    &mut lanes,
                    &mut pre_lane_nodes,
                    &mut lines_out,
                );
                let edge = format!("  {cond_node} -->|\"no\"| {next_d}");
                lines_out.push(edge.clone());
                edge_lines.push(edge);
                *cond_node = next_d.clone();
                tails.push(FlowTail {
                    node: next_d,
                    edge_label: yes_lbl.map(escape_flow_label),
                });
            }
            continue;
        }
        // `else (no)` / `else`
        if lower == "else" || lower.starts_with("else ") || lower.starts_with("else(") {
            let no_lbl = extract_paren_text(trimmed.get(4..).unwrap_or("")).map(escape_flow_label);
            if let Some(ActFrame::If {
                cond_node,
                false_label,
                exits,
                in_else,
            }) = frames.last_mut()
            {
                exits.extend(tails.drain(..));
                *false_label = no_lbl.clone();
                *in_else = true;
                tails.push(FlowTail {
                    node: cond_node.clone(),
                    edge_label: no_lbl,
                });
            }
            continue;
        }
        if lower == "endif" {
            if let Some(ActFrame::If {
                cond_node,
                false_label,
                mut exits,
                in_else,
            }) = frames.pop()
            {
                exits.extend(tails.drain(..));
                if !in_else {
                    exits.push(FlowTail {
                        node: cond_node,
                        edge_label: false_label.or_else(|| Some("no".to_string())),
                    });
                }
                tails = exits;
            }
            continue;
        }
        // `switch (expr)` / `case (val)` / `endswitch`
        if let Some((expr, _)) = parse_act_if(trimmed, "switch") {
            let sw = fresh("SW");
            push_node(
                format!("{sw}{{\"{}\"}}", escape_flow_label(expr)),
                current_lane,
                &mut lanes,
                &mut pre_lane_nodes,
                &mut lines_out,
            );
            connect_to(&sw, &mut tails, &mut lines_out, &mut edge_lines);
            frames.push(ActFrame::Switch {
                cond_node: sw,
                exits: Vec::new(),
                in_case: false,
            });
            has_content = true;
            continue;
        }
        if let Some((val, _)) = parse_act_if(trimmed, "case") {
            if let Some(ActFrame::Switch {
                cond_node,
                exits,
                in_case,
            }) = frames.last_mut()
            {
                if *in_case {
                    exits.extend(tails.drain(..));
                }
                *in_case = true;
                tails.push(FlowTail {
                    node: cond_node.clone(),
                    edge_label: Some(escape_flow_label(val)),
                });
            }
            continue;
        }
        if lower == "endswitch" {
            if let Some(ActFrame::Switch { mut exits, .. }) = frames.pop() {
                exits.extend(tails.drain(..));
                tails = exits;
            }
            continue;
        }
        // `while (cond) is (yes)` / `endwhile (no)`
        if let Some((cond, yes_lbl)) = parse_act_while(trimmed) {
            let w = fresh("W");
            push_node(
                format!("{w}{{\"{}\"}}", escape_flow_label(cond)),
                current_lane,
                &mut lanes,
                &mut pre_lane_nodes,
                &mut lines_out,
            );
            connect_to(&w, &mut tails, &mut lines_out, &mut edge_lines);
            frames.push(ActFrame::While {
                cond_node: w.clone(),
            });
            tails.push(FlowTail {
                node: w,
                edge_label: yes_lbl.map(escape_flow_label),
            });
            has_content = true;
            continue;
        }
        if lower == "endwhile" || lower.starts_with("endwhile ") || lower.starts_with("endwhile(") {
            let exit_lbl =
                extract_paren_text(trimmed.get(8..).unwrap_or("")).map(escape_flow_label);
            if let Some(ActFrame::While { cond_node }) = frames.pop() {
                connect_to(&cond_node, &mut tails, &mut lines_out, &mut edge_lines);
                tails.push(FlowTail {
                    node: cond_node,
                    edge_label: exit_lbl,
                });
            }
            continue;
        }
        // `repeat` ... `repeat while (cond) is (yes) not (no)`
        if lower == "repeat" {
            let r = fresh("R");
            push_node(
                format!("{r}@{{ shape: junction }}"),
                current_lane,
                &mut lanes,
                &mut pre_lane_nodes,
                &mut lines_out,
            );
            connect_to(&r, &mut tails, &mut lines_out, &mut edge_lines);
            frames.push(ActFrame::Repeat {
                entry_node: r.clone(),
            });
            tails.push(FlowTail {
                node: r,
                edge_label: None,
            });
            has_content = true;
            continue;
        }
        if lower.starts_with("repeat while") {
            let cond = extract_paren_text(trimmed.get(12..).unwrap_or("")).unwrap_or("continue?");
            if let Some(ActFrame::Repeat { entry_node }) = frames.pop() {
                let d = fresh("D");
                push_node(
                    format!("{d}{{\"{}\"}}", escape_flow_label(cond)),
                    current_lane,
                    &mut lanes,
                    &mut pre_lane_nodes,
                    &mut lines_out,
                );
                connect_to(&d, &mut tails, &mut lines_out, &mut edge_lines);
                let back_edge = format!("  {d} -->|\"yes\"| {entry_node}");
                lines_out.push(back_edge.clone());
                edge_lines.push(back_edge);
                tails.push(FlowTail {
                    node: d,
                    edge_label: Some("no".to_string()),
                });
            }
            continue;
        }
        // `fork` / `fork again` / `end fork` / `end merge`
        if lower == "fork" {
            let f = fresh("F");
            push_node(
                format!("{f}@{{ shape: fork }}"),
                current_lane,
                &mut lanes,
                &mut pre_lane_nodes,
                &mut lines_out,
            );
            connect_to(&f, &mut tails, &mut lines_out, &mut edge_lines);
            frames.push(ActFrame::Fork {
                fork_node: f.clone(),
                branch_exits: Vec::new(),
            });
            tails.push(FlowTail {
                node: f,
                edge_label: None,
            });
            has_content = true;
            continue;
        }
        if lower == "fork again" {
            if let Some(ActFrame::Fork {
                fork_node,
                branch_exits,
            }) = frames.last_mut()
            {
                branch_exits.extend(tails.drain(..));
                tails.push(FlowTail {
                    node: fork_node.clone(),
                    edge_label: None,
                });
            }
            continue;
        }
        if lower == "end fork" || lower == "end merge" {
            if let Some(ActFrame::Fork {
                mut branch_exits, ..
            }) = frames.pop()
            {
                branch_exits.extend(tails.drain(..));
                let j = fresh("J");
                push_node(
                    format!("{j}@{{ shape: join }}"),
                    current_lane,
                    &mut lanes,
                    &mut pre_lane_nodes,
                    &mut lines_out,
                );
                connect_to(&j, &mut branch_exits, &mut lines_out, &mut edge_lines);
                tails.push(FlowTail {
                    node: j,
                    edge_label: None,
                });
            }
            continue;
        }
        // `split` / `split again` / `end split`
        if lower == "split" {
            let saved = tails.clone();
            frames.push(ActFrame::Split {
                entry_tails: saved,
                branch_exits: Vec::new(),
            });
            continue;
        }
        if lower == "split again" {
            if let Some(ActFrame::Split {
                entry_tails,
                branch_exits,
            }) = frames.last_mut()
            {
                branch_exits.extend(tails.drain(..));
                tails = entry_tails.clone();
            }
            continue;
        }
        if lower == "end split" {
            if let Some(ActFrame::Split {
                mut branch_exits, ..
            }) = frames.pop()
            {
                branch_exits.extend(tails.drain(..));
                tails = branch_exits;
            }
            continue;
        }
        // Notes in activity diagrams (attach to latest tail node if inline, skip multiline cleanly)
        if let Some(rest) = strip_prefix_ci(trimmed, "note ") {
            if let Some((_, text)) = split_colon_outside_quotes(rest) {
                if let Some(last) = tails.last() {
                    let n = fresh("N");
                    push_node(
                        format!(
                            "{n}@{{ shape: comment, label: \"{}\" }}",
                            escape_flow_label(text)
                        ),
                        current_lane,
                        &mut lanes,
                        &mut pre_lane_nodes,
                        &mut lines_out,
                    );
                    let edge = format!("  {} -.- {n}", last.node);
                    lines_out.push(edge.clone());
                    edge_lines.push(edge);
                }
            } else {
                pending_note = true;
            }
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported PlantUML activity diagram syntax.".to_string(),
        });
    }

    while open_partitions > 0 {
        open_partitions -= 1;
        lines_out.push("  end".to_string());
    }
    if !has_content {
        return (None, unsupported);
    }
    if !lanes.is_empty() {
        let mut sw_lines = Vec::new();
        for (lid, ltitle, nodes) in lanes {
            sw_lines.push(format!("  subgraph {lid}[\"{ltitle}\"]"));
            sw_lines.extend(nodes);
            sw_lines.push("  end".to_string());
        }
        sw_lines.extend(edge_lines);
        return (
            Some(prepend_frontmatter_title(
                "swimlane-beta TB",
                title.as_deref(),
                &sw_lines,
            )),
            unsupported,
        );
    }
    (
        Some(prepend_frontmatter_title(
            "flowchart TD",
            title.as_deref(),
            &lines_out,
        )),
        unsupported,
    )
}

fn parse_swimlane_header(line: &str) -> Option<&str> {
    if !line.starts_with('|') || !line.ends_with('|') || line.len() < 3 || line == "|||" {
        return None;
    }
    let inner = &line[1..line.len() - 1];
    // Handle `|#Color|Lane Name|` by taking the segment after the second `|`.
    let name = if let Some(after_col) = inner.strip_prefix('#')
        && let Some((_, rest)) = after_col.split_once('|')
    {
        rest.trim()
    } else {
        inner.trim()
    };
    (!name.is_empty()).then_some(name)
}

/// Match `:action;` or `#Color:action;` and return `(optional_color_token, after_colon)`.
fn split_activity_action_prefix(line: &str) -> Option<(Option<&str>, &str)> {
    if let Some(after) = line.strip_prefix(':') {
        return Some((None, after));
    }
    if line.starts_with('#')
        && let Some(colon) = line.find(':')
    {
        let col = line[..colon].trim();
        if col.len() > 1 && !col.contains(char::is_whitespace) {
            return Some((Some(col), &line[colon + 1..]));
        }
    }
    None
}

fn is_legacy_activity(source: &str) -> bool {
    let mut saw_star_paren = false;
    let mut saw_new_syntax = false;
    for raw in source.lines() {
        let t = raw.trim();
        let lower = t.to_ascii_lowercase();
        if lower == "start" || lower == "stop" || (t.starts_with(':') && t.ends_with(';')) {
            saw_new_syntax = true;
        }
        if t.starts_with("(*)") || t.contains("--> (*)") {
            saw_star_paren = true;
        }
    }
    saw_star_paren && !saw_new_syntax
}

/// Convert legacy PlantUML Activity syntax (`(*) --> "Step"`, `--> [yes] "A"`, `===B1===`, `--> (*)`).
fn convert_plantuml_activity_legacy(
    source: &str,
) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut lines_out: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_enduml = false;
    let mut node_map: HashMap<String, String> = HashMap::new();
    let mut last_node: Option<String> = None;
    let mut next_id: usize = 1;
    let mut has_content = false;

    let mut resolve_legacy_node = |raw: &str,
                                   is_target: bool,
                                   node_map: &mut HashMap<String, String>,
                                   lines: &mut Vec<String>|
     -> String {
        let s = raw.trim();
        if s == "(*)" {
            let id = if is_target {
                format!("E{next_id}")
            } else {
                format!("S{next_id}")
            };
            next_id += 1;
            let shape = if is_target { "stop" } else { "start" };
            lines.push(format!("  {id}@{{ shape: {shape} }}"));
            return id;
        }
        if s.starts_with("===") && s.ends_with("===") && s.len() > 6 {
            let key = s.trim_matches('=').trim();
            if let Some(id) = node_map.get(key) {
                return id.clone();
            }
            let id = format!("B{next_id}");
            next_id += 1;
            node_map.insert(key.to_string(), id.clone());
            lines.push(format!("  {id}@{{ shape: fork }}"));
            return id;
        }
        if let Some(rest) = strip_prefix_ci(s, "if ") {
            let cond = rest.trim().trim_end_matches("then").trim();
            let lbl = escape_flow_label(strip_quotes(cond));
            if let Some(id) = node_map.get(&lbl) {
                return id.clone();
            }
            let id = format!("D{next_id}");
            next_id += 1;
            node_map.insert(lbl.clone(), id.clone());
            lines.push(format!("  {id}{{\"{lbl}\"}}"));
            return id;
        }
        let lbl = escape_flow_label(strip_quotes(s));
        if let Some(id) = node_map.get(&lbl) {
            return id.clone();
        }
        let id = format!("A{next_id}");
        next_id += 1;
        node_map.insert(lbl.clone(), id.clone());
        lines.push(format!("  {id}(\"{lbl}\")"));
        id
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
        if lower == "endif" || lower == "else" {
            continue;
        }
        let norm = normalize_directional_arrows(trimmed);
        if let Some((lhs_raw, rhs_raw)) = norm.split_once("-->").or_else(|| norm.split_once("->")) {
            let lhs_trim = lhs_raw.trim();
            let from_id = if lhs_trim.is_empty() {
                if let Some(prev) = last_node.clone() {
                    prev
                } else {
                    resolve_legacy_node("(*)", false, &mut node_map, &mut lines_out)
                }
            } else {
                resolve_legacy_node(lhs_trim, false, &mut node_map, &mut lines_out)
            };
            let mut rhs_trim = rhs_raw.trim();
            let mut edge_lbl: Option<String> = None;
            if let Some(after_b) = rhs_trim.strip_prefix('[')
                && let Some(close) = after_b.find(']')
            {
                let lbl = escape_flow_label(&after_b[..close]);
                if !lbl.is_empty() {
                    edge_lbl = Some(lbl);
                }
                rhs_trim = after_b[close + 1..].trim();
            }
            let to_id = resolve_legacy_node(rhs_trim, true, &mut node_map, &mut lines_out);
            match edge_lbl {
                Some(lbl) => lines_out.push(format!(
                    "  {from_id} -->|\"{}\"| {to_id}",
                    lbl.replace('|', "/")
                )),
                None => lines_out.push(format!("  {from_id} --> {to_id}")),
            }
            if rhs_trim != "(*)" {
                last_node = Some(to_id);
            }
            has_content = true;
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported legacy PlantUML activity syntax.".to_string(),
        });
    }

    if !has_content {
        return (None, unsupported);
    }
    (
        Some(prepend_frontmatter_title(
            "flowchart TD",
            title.as_deref(),
            &lines_out,
        )),
        unsupported,
    )
}

fn extract_paren_text(s: &str) -> Option<&str> {
    let open = s.find('(')?;
    let rest = &s[open + 1..];
    let close = rest.find(')')?;
    let inner = rest[..close].trim();
    (!inner.is_empty()).then_some(inner)
}

fn parse_act_if<'a>(line: &'a str, kw: &str) -> Option<(&'a str, Option<&'a str>)> {
    let rest = strip_prefix_ci(line, kw)?;
    if !rest.starts_with(char::is_whitespace) && !rest.starts_with('(') {
        return None;
    }
    let open = rest.find('(')?;
    let after_open = &rest[open + 1..];
    let close = after_open.find(')')?;
    let cond = after_open[..close].trim();
    let tail = &after_open[close + 1..];
    let yes_lbl = extract_paren_text(tail);
    Some((cond, yes_lbl))
}

fn parse_act_while(line: &str) -> Option<(&str, Option<&str>)> {
    let rest = strip_prefix_ci(line, "while")?;
    if !rest.starts_with(char::is_whitespace) && !rest.starts_with('(') {
        return None;
    }
    let open = rest.find('(')?;
    let after_open = &rest[open + 1..];
    let close = after_open.find(')')?;
    let cond = after_open[..close].trim();
    let tail = &after_open[close + 1..];
    let yes_lbl = extract_paren_text(tail);
    Some((cond, yes_lbl))
}

// ============================================================================
// 5. PlantUML Component / Deployment Diagram -> Mermaid `flowchart`
// ============================================================================

pub(super) fn convert_plantuml_component(
    source: &str,
) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut lines_out: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut direction = "TB";
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_enduml = false;
    let mut open_groups: usize = 0;
    let mut alias_map: HashMap<String, String> = HashMap::new();
    let mut has_content = false;

    let resolve_comp =
        |raw: &str, alias_map: &mut HashMap<String, String>, lines: &mut Vec<String>| -> String {
            let s = raw.trim();
            if let Some(inner) = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                let lbl = escape_flow_label(strip_quotes(inner));
                if let Some(id) = alias_map.get(&lbl) {
                    return id.clone();
                }
                let id = safe_id(&lbl);
                alias_map.insert(lbl.clone(), id.clone());
                lines.push(format!("  {id}[\"{lbl}\"]"));
                return id;
            }
            let unq = strip_quotes(s);
            if let Some(id) = alias_map.get(unq) {
                return id.clone();
            }
            safe_id(unq)
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
        if lower == "left to right direction" {
            direction = "LR";
            continue;
        }
        if lower == "top to bottom direction" {
            direction = "TB";
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "title ") {
            let t = sanitize_label(rest);
            if !t.is_empty() {
                title = Some(t);
            }
            continue;
        }
        if trimmed == "}" {
            if open_groups > 0 {
                open_groups -= 1;
                lines_out.push(format!("{}end", "  ".repeat(open_groups + 1)));
            }
            continue;
        }
        // Container or leaf element declaration
        if let Some((kind, rest)) = [
            ("component ", "component"),
            ("node ", "node"),
            ("cloud ", "cloud"),
            ("database ", "database"),
            ("storage ", "storage"),
            ("artifact ", "artifact"),
            ("package ", "package"),
            ("folder ", "folder"),
            ("frame ", "frame"),
            ("rectangle ", "rectangle"),
            ("card ", "card"),
            ("agent ", "agent"),
            ("actor ", "actor"),
            ("interface ", "interface"),
            ("queue ", "queue"),
        ]
        .iter()
        .find_map(|&(kw, k)| strip_prefix_ci(trimmed, kw).map(|r| (k, r)))
        {
            let opens = rest.trim_end().ends_with('{');
            let head = rest.trim_end().trim_end_matches('{').trim();
            let (head_clean, fill_hex, had_color) = split_and_normalize_color(head);
            if had_color && (opens || fill_hex.is_none()) {
                unsupported.push(PlantUmlUnsupported {
                    line: line_no,
                    kind: "styling",
                    source: excerpt(trimmed, 120),
                    reason: "Element color is not preserved.".to_string(),
                });
            }
            let (head_no_st, _) = strip_stereotype(head_clean);
            let normalized = if let Some((lhs, rhs)) = split_as_outside_quotes(head_no_st) {
                let lhs_unb = lhs
                    .trim()
                    .strip_prefix('[')
                    .and_then(|r| r.strip_suffix(']'))
                    .unwrap_or(lhs.trim());
                format!("\"{}\" as {}", strip_quotes(lhs_unb), rhs.trim())
            } else {
                let unb = head_no_st
                    .strip_prefix('[')
                    .and_then(|r| r.strip_suffix(']'))
                    .unwrap_or(head_no_st);
                if unb != head_no_st {
                    format!("\"{}\"", strip_quotes(unb))
                } else {
                    head_no_st.to_string()
                }
            };
            let (id, label) = parse_uml_entity_head(&normalized);
            let lbl = escape_flow_label(label.as_deref().unwrap_or(&id));
            alias_map.insert(lbl.clone(), id.clone());
            alias_map.insert(id.clone(), id.clone());
            let ind = "  ".repeat(open_groups + 1);
            if opens {
                lines_out.push(format!("{ind}subgraph {id}[\"{lbl}\"]"));
                open_groups += 1;
            } else {
                let node_decl = match kind {
                    "database" | "storage" => format!("{id}[(\"{lbl}\")]"),
                    "interface" => format!("{id}((\"{lbl}\"))"),
                    "cloud" | "actor" => format!("{id}([\"{lbl}\"])"),
                    "queue" => format!("{id}@{{ shape: h-cyl, label: \"{lbl}\" }}"),
                    "artifact" => format!("{id}@{{ shape: doc, label: \"{lbl}\" }}"),
                    "card" => format!("{id}@{{ shape: card, label: \"{lbl}\" }}"),
                    _ => format!("{id}[\"{lbl}\"]"),
                };
                lines_out.push(format!("{ind}{node_decl}"));
                if let Some(hex) = fill_hex {
                    lines_out.push(format!("{ind}style {id} fill:{hex}"));
                }
            }
            has_content = true;
            continue;
        }
        // Standalone `[Comp] as C`
        if trimmed.starts_with('[') && split_as_outside_quotes(trimmed).is_some() {
            let (lhs, rhs) = split_as_outside_quotes(trimmed).unwrap();
            let inner = lhs
                .trim()
                .strip_prefix('[')
                .and_then(|r| r.strip_suffix(']'))
                .unwrap_or(lhs.trim());
            let lbl = escape_flow_label(strip_quotes(inner));
            let id = safe_id(rhs.trim());
            alias_map.insert(lbl.clone(), id.clone());
            alias_map.insert(id.clone(), id.clone());
            lines_out.push(format!("{}{id}[\"{lbl}\"]", "  ".repeat(open_groups + 1)));
            has_content = true;
            continue;
        }
        // Connections (`A --> B : HTTP`, `A ..> B : uses`, `[Web] --> [API]`)
        let norm = normalize_directional_arrows(trimmed);
        if let Some((lhs_raw, op, rhs_raw, label_raw)) = split_component_link(&norm) {
            let lhs = resolve_comp(lhs_raw, &mut alias_map, &mut lines_out);
            let rhs = resolve_comp(rhs_raw, &mut alias_map, &mut lines_out);
            let (from, to, arrow) = match op {
                "<--" | "<-" => (rhs, lhs, "-->"),
                "<.." => (rhs, lhs, "-.->"),
                "..>" => (lhs, rhs, "-.->"),
                ".." => (lhs, rhs, "-.-"),
                "--" => (lhs, rhs, "---"),
                _ => (lhs, rhs, "-->"),
            };
            let ind = "  ".repeat(open_groups + 1);
            match label_raw.map(escape_flow_label).filter(|l| !l.is_empty()) {
                Some(lbl) => {
                    let clean = lbl.replace('|', "/");
                    lines_out.push(format!("{ind}{from} {arrow}|\"{clean}\"| {to}"));
                }
                None => lines_out.push(format!("{ind}{from} {arrow} {to}")),
            }
            has_content = true;
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported PlantUML component/deployment syntax.".to_string(),
        });
    }

    while open_groups > 0 {
        open_groups -= 1;
        lines_out.push(format!("{}end", "  ".repeat(open_groups + 1)));
    }
    if !has_content {
        return (None, unsupported);
    }
    (
        Some(prepend_frontmatter_title(
            &format!("flowchart {direction}"),
            title.as_deref(),
            &lines_out,
        )),
        unsupported,
    )
}

fn split_component_link(line: &str) -> Option<(&str, &'static str, &str, Option<&str>)> {
    let (expr, label) = match split_colon_outside_quotes(line) {
        Some((e, l)) => (e.trim(), Some(l.trim())),
        None => (line.trim(), None),
    };
    const OPS: &[(&str, &str)] = &[
        ("..>", "..>"),
        ("<..", "<.."),
        ("-->", "-->"),
        ("<--", "<--"),
        ("->", "-->"),
        ("<-", "<--"),
        ("..", ".."),
        ("--", "--"),
    ];
    for &(pat, mapped) in OPS {
        let mut in_quotes = false;
        let mut bracket_depth = 0usize;
        let bytes = expr.as_bytes();
        for i in 0..bytes.len() {
            match bytes[i] {
                b'"' => in_quotes = !in_quotes,
                b'[' if !in_quotes => bracket_depth += 1,
                b']' if !in_quotes => bracket_depth = bracket_depth.saturating_sub(1),
                _ => {}
            }
            if !in_quotes
                && bracket_depth == 0
                && expr.is_char_boundary(i)
                && expr[i..].starts_with(pat)
            {
                let lhs = expr[..i].trim();
                let rhs = expr[i + pat.len()..].trim();
                if !lhs.is_empty() && !rhs.is_empty() {
                    return Some((lhs, mapped, rhs, label));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::markdown::{export_presentation_to_markdown, read_markdown};
    use crate::mermaid::{MermaidOutput, render_plantuml};

    #[test]
    fn renders_plantuml_mindmap_and_wbs_via_existing_renderers() {
        let mm = "\
@startmindmap
* プロジェクト計画
** 要件定義
*** 機能要件
*** 非機能要件
** 設計
@endmindmap
";
        let res = render_plantuml(mm);
        assert!(res.is_lossless(), "unexpected diagnostics: {:?}", res.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res.output else {
            panic!("expected SVG for mindmap");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        assert!(svg.contains("プロジェクト計画") && svg.contains("機能要件"), "{svg}");

        let wbs = "\
@startwbs
* システム刷新
** 基盤構築
*** ネットワーク
** アプリ移行
@endwbs
";
        let res_wbs = render_plantuml(wbs);
        assert!(res_wbs.is_lossless(), "unexpected diagnostics: {:?}", res_wbs.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_wbs.output else {
            panic!("expected SVG for WBS");
        };
        let svg_wbs = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_wbs).is_ok(), "invalid SVG");
        assert!(svg_wbs.contains("システム刷新") && svg_wbs.contains("ネットワーク"), "{svg_wbs}");
    }

    #[test]
    fn renders_plantuml_gantt_via_existing_gantt_renderer() {
        let gantt = "\
@startgantt
title 導入スケジュール
Project starts 2026-04-01
saturday are closed
sunday are closed
-- 設計 --
[基本設計] lasts 5 days
[詳細設計] lasts 5 days and starts at [基本設計]'s end
-- 検証 --
[結合テスト] lasts 3 days
[詳細設計] -> [結合テスト]
[リリース判定] happens at [結合テスト]'s end
@endgantt
";
        let res = render_plantuml(gantt);
        assert!(res.is_lossless(), "unexpected diagnostics: {:?}", res.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res.output else {
            panic!("expected SVG for gantt");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        assert!(svg.contains("導入スケジュール"), "{svg}");
        assert!(svg.contains("基本設計") && svg.contains("結合テスト") && svg.contains("リリース判定"), "{svg}");
    }

    #[test]
    fn renders_plantuml_activity_and_component_via_existing_flowchart_renderer() {
        let activity = "\
@startuml
title 承認フロー
start
:申請書を作成;
if (金額 >= 10万円?) then (はい)
  :部長承認;
else (いいえ)
  :課長承認;
endif
fork
  :会計連携;
fork again
  :通知送信;
end fork
stop
@enduml
";
        let res_act = render_plantuml(activity);
        assert!(res_act.is_lossless(), "unexpected diagnostics: {:?}", res_act.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_act.output else {
            panic!("expected SVG for activity");
        };
        let svg_act = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_act).is_ok(), "invalid SVG");
        assert!(svg_act.contains("申請書を作成") && svg_act.contains("部長承認") && svg_act.contains("会計連携"), "{svg_act}");

        let comp = "\
@startuml
left to right direction
cloud \"インターネット\" as Net {
  component \"Web UI\" as UI
}
node \"アプリサーバー\" as App {
  component \"API\" as API
  database \"PostgreSQL\" as DB
  queue \"ジョブキュー\" as Q
}
UI --> API : HTTPS
API ..> DB : SQL
API --> Q : enqueue
@enduml
";
        let res_comp = render_plantuml(comp);
        assert!(res_comp.is_lossless(), "unexpected diagnostics: {:?}", res_comp.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_comp.output else {
            panic!("expected SVG for component diagram");
        };
        let svg_comp = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_comp).is_ok(), "invalid SVG");
        assert!(
            svg_comp.contains("インターネット")
                && svg_comp.contains("PostgreSQL")
                && svg_comp.contains("ジョブキュー")
                && svg_comp.contains("HTTPS"),
            "{svg_comp}"
        );
    }

    #[test]
    fn markdown_round_trips_step3_plantuml_blocks_as_svg() {
        let md = "\
# 計画書

```plantuml
@startmindmap
* 全体計画
** 要件
** 実装
@endmindmap
```

```puml
@startuml
start
:データ確認: 1次チェック;
stop
@enduml
```
";
        let pres = read_markdown(md.as_bytes()).expect("read_markdown");
        let svg_count = pres
            .media
            .iter()
            .filter(|m| m.format == ibis_doc_model::MediaFormat::Svg)
            .count();
        assert_eq!(svg_count, 2, "expected 2 rendered PlantUML SVGs");
        let saved = export_presentation_to_markdown(&pres).expect("export_presentation_to_markdown");
        assert!(saved.contains("```plantuml\n@startmindmap"), "{saved}");
        assert!(saved.contains("```puml\n@startuml\nstart"), "{saved}");
        assert!(saved.contains(":データ確認: 1次チェック;"), "{saved}");
    }

    #[test]
    fn renders_activity_swimlanes_switch_split_and_legacy_syntax() {
        let swim = "\
@startuml
|営業部|
start
:受注入力;
|#AntiqueWhite|システム|
switch (在庫状況)
case (あり)
  :在庫引当;
case (なし)
  :発注手配;
endswitch
|営業部|
split
  :納品書発行;
split again
  :請求書発行;
end split
stop
@enduml
";
        let res = render_plantuml(swim);
        assert!(res.is_lossless(), "unexpected diagnostics: {:?}", res.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res.output else {
            panic!("expected SVG for swimlane activity diagram");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        assert!(svg.contains("営業部") && svg.contains("システム"), "{svg}");
        assert!(svg.contains("在庫引当") && svg.contains("請求書発行"), "{svg}");

        let legacy = "\
@startuml
(*) --> \"初期化\"
\"初期化\" --> if \"設定あり?\" then
  --> [はい] \"本処理\"
  --> (*)
else
  --> [いいえ] \"デフォルト適用\"
  --> (*)
endif
@enduml
";
        let res_leg = render_plantuml(legacy);
        assert!(res_leg.is_lossless(), "unexpected diagnostics: {:?}", res_leg.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_leg.output else {
            panic!("expected SVG for legacy activity diagram");
        };
        let svg_leg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_leg).is_ok(), "invalid SVG");
        assert!(svg_leg.contains("初期化") && svg_leg.contains("デフォルト適用"), "{svg_leg}");
    }
}
