// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `agentflow-beta`: agent workflows. The syntax is a flowchart with agent-specific node
//! shapes (`input`, `task`, `tool`, `decision`, `refdoc`, `action`), `flow … end` groups,
//! a `global … end` block for shared nodes, `connector` declarations and metadata
//! attributes. It is rewritten into flowchart source and drawn by the flowchart renderer.

use super::*;

/// The flowchart shape an agentflow `shape:` maps to.
fn flow_shape(shape: &str) -> &'static str {
    match shape {
        "input" => "lean-r",
        "tool" => "subproc",
        "decision" => "diam",
        "refdoc" => "doc",
        "action" => "hex",
        _ => "rect",
    }
}

/// `x["X"]@{ shape: tool, connectorRef: "…" }` → `x["X"]@{ shape: subproc }`; metadata-only
/// statements (`x@{ protocol: … }`) return `None`, `x@{ view: collapsed }` is kept.
fn rewrite_attrs(line: &str) -> Option<String> {
    let Some(at) = line.find("@{") else { return Some(line.to_string()) };
    let close = at + line[at..].find('}')?;
    let head = &line[..at];
    let attrs = parse_brace_attrs(&line[at + 2..close]);
    let tail = &line[close + 1..];
    let get = |key: &str| attrs.iter().find(|(k, _)| k == key).map(|(_, v)| unquote(v));
    let is_bare_id = head.trim().chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    if let Some(view) = get("view") {
        return Some(format!("{head}@{{ view: {view} }}{tail}"));
    }
    match get("shape") {
        Some(shape) => {
            // The flowchart parser takes `id@{ shape, label }`, not `id["X"]@{ shape }`.
            let head = head.trim_end();
            let (id, label) = match (head.find('['), head.ends_with(']')) {
                (Some(open), true) => (head[..open].trim(), Some(unquote(head[open + 1..head.len() - 1].trim()))),
                _ => (head.trim(), None),
            };
            let label = label.map_or(String::new(), |l| format!(", label: \"{}\"", l.replace('"', "#quot;")));
            Some(format!("{id}@{{ shape: {}{label} }}{tail}", flow_shape(&shape)))
        }
        None if is_bare_id && tail.trim().is_empty() => None,
        None => Some(format!("{head}{tail}")),
    }
}

/// Rewrites agentflow source into flowchart source.
pub(super) fn agentflow_to_flowchart(source: &str) -> String {
    let mut lines = source.lines();
    let header = lines.next().unwrap_or("");
    let dir = header.split_whitespace().nth(1).unwrap_or("TB");
    let mut out = format!("flowchart {dir}\n");
    // true for a `global` block, whose `end` is dropped.
    let mut blocks: Vec<bool> = Vec::new();
    // Ids declared in `global`, and links to them written inside a flow: those links move to
    // the top level so the shared node is not pulled into the first flow that uses it.
    let mut globals: Vec<String> = Vec::new();
    let mut deferred: Vec<String> = Vec::new();
    for raw in lines {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if l == "global" {
            blocks.push(true);
            continue;
        }
        if l == "end" {
            if blocks.pop() != Some(true) {
                out.push_str("end\n");
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("flow ") {
            blocks.push(false);
            out.push_str(&format!("subgraph {}\n", rest.trim()));
            continue;
        }
        let l = l.strip_prefix("connector ").unwrap_or(l);
        let in_global = blocks.last() == Some(&true);
        if in_global {
            let id: String = l.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
            if !id.is_empty() {
                globals.push(id);
            }
        }
        let Some(stmt) = rewrite_attrs(l) else { continue };
        let in_flow = blocks.iter().any(|g| !g);
        let mentions_global = stmt.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).any(|w| globals.iter().any(|g| g == w));
        if in_flow && !in_global && mentions_global {
            deferred.push(stmt);
            continue;
        }
        out.push_str(&stmt);
        out.push('\n');
    }
    for stmt in deferred {
        out.push_str(&stmt);
        out.push('\n');
    }
    out
}

pub(super) fn render_agentflow_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let spacing = |key, default| {
        config.option(key)
            .and_then(px_number)
            .filter(|value| (0.0..=1000.0).contains(value))
            .map_or(default, |value| value as f32)
    };
    render_flowchart_svg_with_spacing(
        &agentflow_to_flowchart(source),
        spacing("agentflow.nodeSpacing", FLOW_NODE_GAP),
        spacing("agentflow.rankSpacing", FLOW_RANK_GAP),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn agentflow_rewrites_flows_shapes_and_metadata() {
        let src = "agentflow-beta LR\n  connector gh[\"GitHub API\"]\n  gh@{ protocol: \"http\" }\n  global\n    c[\"Corpus\"]@{ shape: refdoc }\n  end\n  flow w[\"Writer\"]\n    t[\"create\"]@{ shape: tool, connectorRef: \"gh.x\" }\n    i[\"in\"]@{ shape: input, value: \"x\" }\n  end\n  w@{ view: \"collapsed\" }\n  w@{ instruction: \"Do it\" }\n  i --> t";
        assert_eq!(
            agentflow_to_flowchart(src),
            "flowchart LR\ngh[\"GitHub API\"]\nc@{ shape: doc, label: \"Corpus\" }\nsubgraph w[\"Writer\"]\nt@{ shape: subproc, label: \"create\" }\ni@{ shape: lean-r, label: \"in\" }\nend\nw@{ view: collapsed }\ni --> t\n"
        );
    }

    #[test]
    fn agentflow_global_links_stay_outside_flows() {
        let src = "agentflow-beta TB\n  global\n    corpus[\"Corpus\"]@{ shape: refdoc }\n  end\n  flow s[\"S\"]\n    sum[\"Sum\"]@{ shape: task }\n    sum -.- corpus\n  end";
        assert!(agentflow_to_flowchart(src).ends_with("end\nsum -.- corpus\n"));
    }

    #[test]
    fn agentflow_spacing_respects_direction_and_scope() {
        for (direction, horizontal, grouped) in [("TB", false, false), ("RL", true, true)] {
            let mut body = format!("agentflow {direction}\n");
            if grouped {
                body.push_str("flow review[Review]\n");
            }
            body.push_str("a[Start] --> b[First]\na --> c[Second]\n");
            if grouped {
                body.push_str("end\n");
            }
            let gaps = |svg: &str| {
                let (rank, node): (fn(&str, &str) -> f32, fn(&str, &str) -> f32) =
                    if horizontal { (text_x, text_y) } else { (text_y, text_x) };
                ((rank(svg, "First") - rank(svg, "Start")).abs(),
                 (node(svg, "First") - node(svg, "Second")).abs())
            };
            let (rank, node) = gaps(&svg_of(&body));
            let render = |key, value| svg_of(&format!("---\nconfig:\n  agentflow:\n    {key}: {value}\n---\n{body}"));
            let (wider_rank, same_node) = gaps(&render("rankSpacing", "104"));
            let (same_rank, wider_node) = gaps(&render("nodeSpacing", "74"));
            assert!((wider_rank - rank - 40.0).abs() < 0.2);
            assert!((wider_node - node - 40.0).abs() < 0.2);
            assert!((same_rank - rank).abs() < 0.2);
            assert!((same_node - node).abs() < 0.2);
            assert_eq!(gaps(&render("nodeSpacing", "-1")), (rank, node));
            let flowchart = body.replace("agentflow", "flowchart").replace("flow review", "subgraph review");
            let scoped = format!("---\nconfig:\n  agentflow:\n    nodeSpacing: 74\n---\n{flowchart}");
            assert_eq!(svg_of(&scoped), svg_of(&flowchart));
        }
    }

    #[test]
    fn agentflow_svg_draws_groups_and_shapes() {
        let svg = svg_of("agentflow-beta TB\n  flow r[\"Review Agent\"]\n    a[\"Analyse\"]@{ shape: task }\n    ok[\"Clean?\"]@{ shape: decision }\n    a --> ok\n  end");
        assert!(svg.contains(">Review Agent<") && svg.contains(">Analyse<") && svg.contains(">Clean?<"));
        assert!(text_y(&svg, "Clean?") > text_y(&svg, "Analyse"));
    }
}
