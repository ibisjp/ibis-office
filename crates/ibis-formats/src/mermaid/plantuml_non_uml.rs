// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Non-UML PlantUML diagram converters frequently used in specifications:
//! - C4-PlantUML (`Person`/`System`/`Container`/`Component`/`*_Boundary`/`Rel*`) -> `c4.rs`
//! - `@startjson` / `@startyaml` -> `treeview.rs` (`treeView-beta`)
//! - `@startebnf` -> `railroad.rs` (`railroad-ebnf-beta`)
//! - `@startnwdiag` -> `flowchart.rs` (`flowchart TB`)
//! - `@startchronology` -> `timeline.rs` (`timeline`)

use std::collections::HashMap;

use super::plantuml::{
    PlantUmlUnsupported, excerpt, sanitize_label, split_colon_outside_quotes, strip_block_comments,
    strip_prefix_ci, strip_quotes,
};
use super::plantuml_uml::{
    UmlSkip, advance_skip, handle_common_ignored_or_unsupported, prepend_frontmatter_title, safe_id,
};

// ============================================================================
// 1. C4-PlantUML -> Mermaid `C4Context` / `C4Container` / `C4Component` / ...
// ============================================================================

/// Detect if a `@startuml` source is a C4-PlantUML diagram.
pub(super) fn is_c4_plantuml(source: &str) -> bool {
    let mut in_comment = false;
    for raw in source.lines() {
        let cleaned = strip_block_comments(raw, &mut in_comment);
        let t = cleaned.trim();
        if t.is_empty() || t.starts_with('\'') {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("!include") && lower.contains("c4_") {
            return true;
        }
        if is_c4_call_prefix(t) {
            return true;
        }
    }
    false
}

fn is_c4_call_prefix(trimmed: &str) -> bool {
    const CALLS: &[&str] = &[
        "Person(",
        "Person_Ext(",
        "System(",
        "System_Ext(",
        "SystemDb(",
        "SystemDb_Ext(",
        "SystemQueue(",
        "SystemQueue_Ext(",
        "Container(",
        "Container_Ext(",
        "ContainerDb(",
        "ContainerDb_Ext(",
        "ContainerQueue(",
        "ContainerQueue_Ext(",
        "Component(",
        "Component_Ext(",
        "ComponentDb(",
        "ComponentDb_Ext(",
        "ComponentQueue(",
        "ComponentQueue_Ext(",
        "Enterprise_Boundary(",
        "System_Boundary(",
        "Container_Boundary(",
        "Deployment_Node(",
        "Deployment_Node_L(",
        "Deployment_Node_R(",
        "Rel(",
        "Rel_D(",
        "Rel_Down(",
        "Rel_U(",
        "Rel_Up(",
        "Rel_L(",
        "Rel_Left(",
        "Rel_R(",
        "Rel_Right(",
        "Rel_Back(",
        "BiRel(",
        "BiRel_D(",
        "BiRel_U(",
        "BiRel_L(",
        "BiRel_R(",
    ];
    CALLS.iter().any(|p| trimmed.starts_with(p))
}

fn c4_header_from_source(source: &str) -> &'static str {
    let lower = source.to_ascii_lowercase();
    if lower.contains("c4_deployment") || source.contains("Deployment_Node(") {
        "C4Deployment"
    } else if lower.contains("c4_dynamic") || source.contains("RelIndex(") {
        "C4Dynamic"
    } else if lower.contains("c4_component") || source.contains("Component(") {
        "C4Component"
    } else if lower.contains("c4_container") || source.contains("Container(") {
        "C4Container"
    } else {
        "C4Context"
    }
}

pub(super) fn convert_plantuml_c4(source: &str) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut lines_out: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_enduml = false;
    let mut has_content = false;

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
        // Standard C4-PlantUML stdlib includes are recognized statically without network/disk IO.
        if lower.starts_with("!include") && lower.contains("c4") {
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
        // Common C4-PlantUML layout/legend macros safely ignored.
        if trimmed.starts_with("LAYOUT_")
            || trimmed.starts_with("SHOW_")
            || trimmed.starts_with("HIDE_")
            || trimmed.starts_with("WITHOUT_")
            || trimmed.starts_with("SetPropertyHeader(")
            || trimmed.starts_with("AddProperty(")
            || trimmed.starts_with("AddElementTag(")
            || trimmed.starts_with("AddRelTag(")
        {
            continue;
        }
        if trimmed == "}" {
            lines_out.push("  }".to_string());
            continue;
        }
        // Normalize `Deployment_Node_L`/`Deployment_Node_R` -> `Deployment_Node`
        // and `Rel_Neighbor` -> `Rel`.
        let normalized = trimmed
            .replace("Deployment_Node_L(", "Deployment_Node(")
            .replace("Deployment_Node_R(", "Deployment_Node(")
            .replace("Rel_Neighbor(", "Rel(")
            .replace("BiRel_Neighbor(", "BiRel(")
            .replace("Rel_Back_Neighbor(", "Rel_Back(");
        if is_c4_call_prefix(&normalized)
            || normalized.starts_with("Boundary(")
            || normalized.starts_with("Node(")
            || normalized.starts_with("Node_L(")
            || normalized.starts_with("Node_R(")
            || normalized.starts_with("UpdateElementStyle(")
            || normalized.starts_with("UpdateRelStyle(")
            || normalized.starts_with("UpdateLayoutConfig(")
        {
            lines_out.push(format!("  {normalized}"));
            has_content = true;
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported C4-PlantUML statement.".to_string(),
        });
    }

    if !has_content {
        return (None, unsupported);
    }
    let header = c4_header_from_source(source);
    let mut body = Vec::new();
    if let Some(t) = title {
        body.push(format!("  title {t}"));
    }
    body.extend(lines_out);
    let mut out = format!("{header}\n");
    for line in body {
        out.push_str(&line);
        out.push('\n');
    }
    (Some(out), unsupported)
}

// ============================================================================
// 2. PlantUML JSON (`@startjson`) -> Mermaid `treeView-beta`
// ============================================================================

pub(super) fn convert_plantuml_json(source: &str) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut title: Option<String> = None;
    let mut json_lines: Vec<&str> = Vec::new();
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
        if trimmed.is_empty() {
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
        if json_lines.is_empty() {
            if trimmed.starts_with('\'') || lower.starts_with("#highlight ") {
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
        }
        json_lines.push(raw);
    }

    let payload = json_lines.join("\n");
    let value: serde_json::Value = match serde_json::from_str(&payload) {
        Ok(v) => v,
        Err(err) => {
            unsupported.push(PlantUmlUnsupported {
                line: 1,
                kind: "syntax",
                source: excerpt(payload.trim(), 120),
                reason: format!("Invalid JSON in @startjson: {err}"),
            });
            return (None, unsupported);
        }
    };

    let mut lines_out = Vec::new();
    let root_label = title.as_deref().unwrap_or("JSON");
    emit_json_tree(&value, Some(root_label), 0, &mut lines_out);
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

fn emit_json_tree(
    val: &serde_json::Value,
    key: Option<&str>,
    depth: usize,
    out: &mut Vec<String>,
) {
    let ind = "    ".repeat(depth);
    let esc = |s: &str| sanitize_label(s).replace('"', "'");
    match val {
        serde_json::Value::Object(map) => {
            if let Some(k) = key {
                out.push(format!("{ind}\"{}\"", esc(k)));
            }
            for (k, v) in map {
                emit_json_tree(v, Some(k), depth + usize::from(key.is_some()), out);
            }
        }
        serde_json::Value::Array(arr) => {
            if let Some(k) = key {
                out.push(format!("{ind}\"{} [{}]\"", esc(k), arr.len()));
            }
            for (i, item) in arr.iter().enumerate() {
                let tag = format!("[{i}]");
                emit_json_tree(item, Some(&tag), depth + usize::from(key.is_some()), out);
            }
        }
        scalar => {
            let v_str = match scalar {
                serde_json::Value::String(s) => format!("'{}'", esc(s)),
                serde_json::Value::Null => "null".to_string(),
                other => other.to_string(),
            };
            match key {
                Some(k) => out.push(format!("{ind}\"{}: {v_str}\"", esc(k))),
                None => out.push(format!("{ind}\"{v_str}\"")),
            }
        }
    }
}

// ============================================================================
// 3. PlantUML YAML (`@startyaml`) -> Mermaid `treeView-beta`
// ============================================================================

pub(super) fn convert_plantuml_yaml(source: &str) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut title: Option<String> = None;
    let mut lines_out: Vec<String> = Vec::new();
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_end = false;
    let mut saw_body = false;

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
        if trimmed.is_empty() || trimmed == "---" || trimmed == "..." {
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
        if !saw_body {
            if trimmed.starts_with('\'') || lower.starts_with("#highlight ") {
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
        }
        if trimmed.starts_with('#') {
            continue;
        }
        let raw_indent = cleaned.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        let level = raw_indent / 2 + 1;
        let label = sanitize_label(trimmed.trim_end_matches(':')).replace('"', "'");
        if !label.is_empty() {
            if !saw_body {
                let root = title.as_deref().unwrap_or("YAML").replace('"', "'");
                lines_out.push(format!("\"{root}\""));
                saw_body = true;
            }
            lines_out.push(format!("{}\"{label}\"", "    ".repeat(level)));
        }
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
// 4. PlantUML EBNF (`@startebnf`) -> Mermaid `railroad-ebnf-beta`
// ============================================================================

pub(super) fn convert_plantuml_ebnf(source: &str) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut title: Option<String> = None;
    let mut lines_out: Vec<String> = Vec::new();
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
        // Strip inline EBNF `(* ... *)` comments before passing rule to `railroad-ebnf-beta`.
        let rule_line = strip_ebnf_comments(trimmed);
        if !rule_line.is_empty() {
            lines_out.push(format!("  {rule_line}"));
        }
    }

    if lines_out.is_empty() {
        return (None, unsupported);
    }
    (
        Some(prepend_frontmatter_title(
            "railroad-ebnf-beta",
            title.as_deref(),
            &lines_out,
        )),
        unsupported,
    )
}

fn strip_ebnf_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find("(*") {
        out.push_str(&rest[..open]);
        if let Some(close) = rest[open + 2..].find("*)") {
            rest = &rest[open + 2 + close + 2..];
        } else {
            return out.trim().to_string();
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

// ============================================================================
// 5. PlantUML Network Diagram (`@startnwdiag`) -> Mermaid `flowchart TB`
// ============================================================================

struct NwNetwork {
    id: String,
    name: String,
    address: Option<String>,
    members: Vec<(String, Option<String>)>, // (node_id, optional ip)
}

pub(super) fn convert_plantuml_nwdiag(source: &str) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut title: Option<String> = None;
    let mut networks: Vec<NwNetwork> = Vec::new();
    let mut node_labels: HashMap<String, String> = HashMap::new();
    let mut peer_links: Vec<(String, String)> = Vec::new();
    let mut in_comment = false;
    let mut skip_until: Option<UmlSkip> = None;
    let mut saw_end = false;
    let mut cur_net: Option<usize> = None;
    let mut in_nwdiag = false;
    let mut in_group = false;

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
        if trimmed.is_empty() || trimmed.starts_with('\'') || trimmed.starts_with("//") {
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
        if lower == "nwdiag {" || lower == "nwdiag{" {
            in_nwdiag = true;
            continue;
        }
        if trimmed == "}" {
            if cur_net.is_some() {
                cur_net = None;
            } else if in_group {
                in_group = false;
            } else if in_nwdiag {
                in_nwdiag = false;
            }
            continue;
        }
        if lower.starts_with("group") && trimmed.ends_with('{') {
            in_group = true;
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "network ")
            && trimmed.ends_with('{')
        {
            let name = sanitize_label(strip_quotes(rest.trim_end_matches('{').trim()));
            let id = format!("net_{}_{}", networks.len() + 1, safe_id(&name));
            networks.push(NwNetwork {
                id,
                name,
                address: None,
                members: Vec::new(),
            });
            cur_net = Some(networks.len() - 1);
            continue;
        }
        if let Some(ni) = cur_net {
            let stmt = trimmed.trim_end_matches(';').trim();
            if let Some(rest) = strip_prefix_ci(stmt, "address")
                && let Some((_, val)) = rest.split_once('=')
            {
                networks[ni].address = Some(sanitize_label(strip_quotes(val.trim())));
                continue;
            }
            if lower.starts_with("color") || lower.starts_with("width") {
                continue;
            }
            // Host declaration: `web01 [address = "210.x.x.1", description = "Web"];`
            let (raw_name, attrs) = match stmt.find('[') {
                Some(open) => (
                    stmt[..open].trim(),
                    stmt[open + 1..].trim_end_matches(']').trim(),
                ),
                None => (stmt, ""),
            };
            let node_name = sanitize_label(strip_quotes(raw_name));
            if !node_name.is_empty() {
                let nid = safe_id(&node_name);
                let addr = extract_nw_attr(attrs, "address");
                let desc = extract_nw_attr(attrs, "description");
                node_labels
                    .entry(nid.clone())
                    .or_insert_with(|| desc.unwrap_or(node_name));
                networks[ni].members.push((nid, addr));
            }
            continue;
        }
        // Peer link outside a network: `internet -- router;`
        let stmt = trimmed.trim_end_matches(';').trim();
        if let Some((a, b)) = stmt.split_once("--") {
            let a_name = sanitize_label(strip_quotes(a.trim()));
            let b_name = sanitize_label(strip_quotes(b.trim()));
            if !a_name.is_empty() && !b_name.is_empty() {
                let aid = safe_id(&a_name);
                let bid = safe_id(&b_name);
                node_labels.entry(aid.clone()).or_insert(a_name);
                node_labels.entry(bid.clone()).or_insert(b_name);
                peer_links.push((aid, bid));
                continue;
            }
        }
        // Standalone host line inside `group { web01; web02; }`
        if in_group {
            let name = sanitize_label(strip_quotes(stmt));
            if !name.is_empty() {
                let nid = safe_id(&name);
                node_labels.entry(nid).or_insert(name);
            }
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported PlantUML nwdiag syntax.".to_string(),
        });
    }

    if networks.is_empty() && peer_links.is_empty() {
        return (None, unsupported);
    }

    let mut lines_out = Vec::new();
    // Declare each host once at top-level or inside its first network so multi-homed hosts
    // cleanly connect to every network they belong to.
    let mut member_counts: HashMap<String, usize> = HashMap::new();
    for net in &networks {
        for (nid, _) in &net.members {
            *member_counts.entry(nid.clone()).or_insert(0) += 1;
        }
    }
    let mut declared: HashMap<String, bool> = HashMap::new();

    for net in &networks {
        let net_title = match &net.address {
            Some(addr) => format!("{} ({addr})", net.name).replace('"', "'"),
            None => net.name.replace('"', "'"),
        };
        lines_out.push(format!("  subgraph {}[\"{net_title}\"]", net.id));
        let bus_id = format!("{}_bus", net.id);
        lines_out.push(format!("    {bus_id}([\"{net_title}\"])"));
        for (nid, _) in &net.members {
            if member_counts.get(nid).copied().unwrap_or(0) == 1 && !declared.contains_key(nid) {
                let lbl = node_labels
                    .get(nid)
                    .map(|s| s.replace('"', "'"))
                    .unwrap_or_else(|| nid.clone());
                lines_out.push(format!("    {nid}[\"{lbl}\"]"));
                declared.insert(nid.clone(), true);
            }
        }
        lines_out.push("  end".to_string());
    }
    for (nid, lbl) in &node_labels {
        if !declared.contains_key(nid) {
            lines_out.push(format!("  {nid}[\"{}\"]", lbl.replace('"', "'")));
            declared.insert(nid.clone(), true);
        }
    }
    for net in &networks {
        let bus_id = format!("{}_bus", net.id);
        for (nid, addr) in &net.members {
            match addr {
                Some(ip) => lines_out.push(format!(
                    "  {bus_id} ---|\"{}\"| {nid}",
                    ip.replace('"', "'").replace('|', "/")
                )),
                None => lines_out.push(format!("  {bus_id} --- {nid}")),
            }
        }
    }
    for (a, b) in peer_links {
        lines_out.push(format!("  {a} --- {b}"));
    }

    (
        Some(prepend_frontmatter_title(
            "flowchart TB",
            title.as_deref(),
            &lines_out,
        )),
        unsupported,
    )
}

fn extract_nw_attr(attrs: &str, key: &str) -> Option<String> {
    for part in attrs.split(',') {
        if let Some((k, v)) = part.split_once('=')
            && k.trim().eq_ignore_ascii_case(key)
        {
            let val = sanitize_label(strip_quotes(v.trim()));
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

// ============================================================================
// 6. PlantUML Chronology (`@startchronology`) -> Mermaid `timeline`
// ============================================================================

pub(super) fn convert_plantuml_chronology(
    source: &str,
) -> (Option<String>, Vec<PlantUmlUnsupported>) {
    let mut unsupported = Vec::new();
    let mut title: Option<String> = None;
    let mut lines_out: Vec<String> = Vec::new();
    let mut current_period: Option<String> = None;
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
        if trimmed.starts_with("--") && trimmed.ends_with("--") && trimmed.len() >= 4 {
            let sec = sanitize_label(trimmed.trim_matches('-').trim());
            if !sec.is_empty() {
                lines_out.push(format!("  section {sec}"));
            }
            continue;
        }
        // `@2024` / `@2026-04` period header
        if let Some(period) = trimmed.strip_prefix('@') {
            let p = sanitize_label(period);
            if !p.is_empty() {
                current_period = Some(p);
            }
            continue;
        }
        // `[Event] happens 2026-04-01` or `[2024] : Event`
        if let Some(after_b) = trimmed.strip_prefix('[')
            && let Some(close) = after_b.find(']')
        {
            let head = sanitize_label(&after_b[..close]);
            let rest = after_b[close + 1..].trim();
            if let Some((_, ev)) = split_colon_outside_quotes(rest) {
                lines_out.push(format!("  {head} : {}", sanitize_label(ev)));
                current_period = None;
                continue;
            }
            if let Some(after_h) = strip_prefix_ci(rest, "happens ")
                .or_else(|| strip_prefix_ci(rest, "starts "))
                .or_else(|| strip_prefix_ci(rest, "at "))
            {
                let period = sanitize_label(after_h);
                if !period.is_empty() && !head.is_empty() {
                    lines_out.push(format!("  {period} : {head}"));
                    current_period = None;
                    continue;
                }
            }
        }
        // Direct `2024 : Event A : Event B`
        if let Some((lhs, rhs)) = split_colon_outside_quotes(trimmed)
            && !lhs.trim().is_empty()
            && !rhs.trim().is_empty()
        {
            lines_out.push(format!(
                "  {} : {}",
                sanitize_label(lhs),
                sanitize_label(rhs)
            ));
            current_period = None;
            continue;
        }
        // Event line under an active `@period`
        if let Some(p) = current_period.take() {
            lines_out.push(format!("  {p} : {}", sanitize_label(trimmed)));
            continue;
        }
        if !lines_out.is_empty() {
            lines_out.push(format!("    : {}", sanitize_label(trimmed)));
            continue;
        }
        unsupported.push(PlantUmlUnsupported {
            line: line_no,
            kind: "syntax",
            source: excerpt(trimmed, 120),
            reason: "Unsupported PlantUML chronology syntax.".to_string(),
        });
    }

    if lines_out.is_empty() {
        return (None, unsupported);
    }
    let mut out = String::from("timeline\n");
    if let Some(t) = title {
        out.push_str(&format!("  title {t}\n"));
    }
    for line in lines_out {
        out.push_str(&line);
        out.push('\n');
    }
    (Some(out), unsupported)
}

#[cfg(test)]
mod tests {
    use crate::mermaid::{MermaidOutput, render_plantuml};

    #[test]
    fn renders_c4_plantuml_via_existing_c4_renderer() {
        let puml = "\
@startuml
!include <C4/C4_Container.puml>
title 自治体オンライン申請システム
Person(user, \"住民\", \"オンラインで手続きを行う\")
System_Boundary(sys, \"申請プラットフォーム\") {
  Container(web, \"Web フロント\", \"TypeScript\", \"申請フォーム提供\")
  ContainerDb(db, \"申請 DB\", \"PostgreSQL\", \"申請データ保存\")
}
Rel(user, web, \"申請送信\", \"HTTPS\")
Rel(web, db, \"永続化\", \"SQL\")
LAYOUT_WITH_LEGEND()
@enduml
";
        let res = render_plantuml(puml);
        assert!(res.is_lossless(), "unexpected diagnostics: {:?}", res.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res.output else {
            panic!("expected SVG for C4-PlantUML");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        assert!(
            svg.contains("住民") && svg.contains("Web フロント") && svg.contains("PostgreSQL"),
            "{svg}"
        );
    }

    #[test]
    fn renders_plantuml_json_and_yaml_via_treeview_renderer() {
        let json = r#"
@startjson
title API レスポンス
{
  "status": "ok",
  "user": {
    "id": 42,
    "roles": ["admin", "editor"]
  }
}
@endjson
"#;
        let res_j = render_plantuml(json);
        assert!(res_j.is_lossless(), "unexpected diagnostics: {:?}", res_j.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_j.output else {
            panic!("expected SVG for @startjson");
        };
        let svg_j = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_j).is_ok(), "invalid SVG");
        assert!(svg_j.contains("status") && svg_j.contains("admin"), "{svg_j}");

        let yaml = "\
@startyaml
title 設定ファイル
server:
  host: 0.0.0.0
  port: 8080
features:
  - auth
  - audit
@endyaml
";
        let res_y = render_plantuml(yaml);
        assert!(res_y.is_lossless(), "unexpected diagnostics: {:?}", res_y.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_y.output else {
            panic!("expected SVG for @startyaml");
        };
        let svg_y = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_y).is_ok(), "invalid SVG");
        assert!(svg_y.contains("server") && svg_y.contains("8080"), "{svg_y}");
    }

    #[test]
    fn renders_plantuml_ebnf_nwdiag_and_chronology() {
        let ebnf = "\
@startebnf
title 識別子文法
ident = letter, { letter | digit };
letter = \"a\" | \"b\";
@endebnf
";
        let res_e = render_plantuml(ebnf);
        assert!(res_e.is_lossless(), "unexpected diagnostics: {:?}", res_e.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_e.output else {
            panic!("expected SVG for @startebnf");
        };
        let svg_e = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_e).is_ok(), "invalid SVG");
        assert!(svg_e.contains("ident") && svg_e.contains("letter"), "{svg_e}");

        let nw = "\
@startnwdiag
nwdiag {
  network dmz {
    address = \"210.x.x.x/24\"
    web01 [address = \"210.x.x.1\"];
    web02 [address = \"210.x.x.2\"];
  }
  network internal {
    address = \"172.x.x.x/24\"
    web01 [address = \"172.x.x.1\"];
    db01 [address = \"172.x.x.100\"];
  }
}
@endnwdiag
";
        let res_n = render_plantuml(nw);
        assert!(res_n.is_lossless(), "unexpected diagnostics: {:?}", res_n.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_n.output else {
            panic!("expected SVG for @startnwdiag");
        };
        let svg_n = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_n).is_ok(), "invalid SVG");
        assert!(
            svg_n.contains("dmz") && svg_n.contains("web01") && svg_n.contains("db01"),
            "{svg_n}"
        );

        let chrono = "\
@startchronology
title システム刷新沿革
-- 準備期 --
2024 : 現行調査 : 要件定義
-- 移行期 --
@2025
設計・開発
結合テスト
[本番移行] happens 2026
@endchronology
";
        let res_c = render_plantuml(chrono);
        assert!(res_c.is_lossless(), "unexpected diagnostics: {:?}", res_c.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_c.output else {
            panic!("expected SVG for @startchronology");
        };
        let svg_c = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_c).is_ok(), "invalid SVG");
        assert!(
            svg_c.contains("システム刷新沿革")
                && svg_c.contains("要件定義")
                && svg_c.contains("本番移行"),
            "{svg_c}"
        );
    }
}
