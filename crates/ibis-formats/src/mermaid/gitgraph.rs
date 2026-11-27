// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `gitGraph`.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GitCommitType {
    Normal,
    Reverse,
    Highlight,
    Merge,
    CherryPick,
}

pub(super) struct GitCommit {
    pub(super) branch: usize,
    pub(super) id: String,
    pub(super) tag: Option<String>,
    pub(super) kind: GitCommitType,
    pub(super) parent: Option<usize>,
    pub(super) merge_parent: Option<usize>,
    /// The commit a `cherry-pick` copied.
    pub(super) picked_from: Option<usize>,
}

pub(super) struct GitBranch {
    pub(super) name: String,
    /// `branch x order: n`; `None` sorts by definition order between 0 and 1, as mermaid does.
    pub(super) order: Option<f32>,
    pub(super) head: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GitDir {
    Lr,
    Tb,
    Bt,
}

pub(super) fn git_commit_type(line: &str) -> Option<GitCommitType> {
    match extract_git_attr(line, "type:")?.to_ascii_uppercase().as_str() {
        "REVERSE" => Some(GitCommitType::Reverse),
        "HIGHLIGHT" => Some(GitCommitType::Highlight),
        "NORMAL" => Some(GitCommitType::Normal),
        _ => None,
    }
}

pub(super) fn render_git_graph_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let mut lines = source.lines();
    let header = lines.next()?.trim();
    let dir = match header.split_whitespace().nth(1).map(|t| t.trim_end_matches(':').to_ascii_uppercase()).as_deref() {
        Some("TB") => GitDir::Tb,
        Some("BT") => GitDir::Bt,
        _ => GitDir::Lr,
    };
    let main_name = config
        .option("gitGraph.mainBranchName")
        .map(unquote)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "main".to_string());
    let main_order = config.option("gitGraph.mainBranchOrder").and_then(|v| v.trim().parse::<f32>().ok());
    let show_branches = config.flag("gitGraph.showBranches", true);
    let show_labels = config.flag("gitGraph.showCommitLabel", true);
    let rotate_labels = dir == GitDir::Lr && config.flag("gitGraph.rotateCommitLabel", false);
    let parallel = config.flag("gitGraph.parallelCommits", false);

    let mut branches = vec![GitBranch {
        name: main_name,
        order: Some(main_order.unwrap_or(0.0)),
        head: None,
    }];
    let mut current = 0usize;
    let mut commits: Vec<GitCommit> = Vec::new();
    let mut auto_idx = 1usize;
    let next_id = |auto_idx: &mut usize| {
        let s = format!("c{auto_idx}");
        *auto_idx += 1;
        s
    };

    for raw in lines {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        let word = l.split(|c: char| c.is_whitespace()).next().unwrap_or("");
        match word {
            "commit" => {
                let id = extract_git_attr(l, "id:").unwrap_or_else(|| next_id(&mut auto_idx));
                let idx = commits.len();
                commits.push(GitCommit {
                    branch: current,
                    id,
                    tag: extract_git_attr(l, "tag:"),
                    kind: git_commit_type(l).unwrap_or(GitCommitType::Normal),
                    parent: branches[current].head,
                    merge_parent: None,
                    picked_from: None,
                });
                branches[current].head = Some(idx);
            }
            "branch" => {
                let rest = l["branch".len()..].trim();
                let name = unquote(rest.split_whitespace().next().unwrap_or(""));
                if name.is_empty() {
                    continue;
                }
                let order = extract_git_attr(l, "order:").and_then(|o| o.parse::<f32>().ok());
                match branches.iter().position(|b| b.name == name) {
                    Some(pos) => current = pos,
                    None => {
                        let head = branches[current].head;
                        branches.push(GitBranch { name, order, head });
                        current = branches.len() - 1;
                    }
                }
            }
            "checkout" | "switch" => {
                let name = unquote(l[word.len()..].split_whitespace().next().unwrap_or(""));
                if let Some(pos) = branches.iter().position(|b| b.name == name) {
                    current = pos;
                }
            }
            "merge" => {
                let name = unquote(l[word.len()..].split_whitespace().next().unwrap_or(""));
                let Some(src) = branches.iter().position(|b| b.name == name) else {
                    continue;
                };
                let idx = commits.len();
                commits.push(GitCommit {
                    branch: current,
                    id: extract_git_attr(l, "id:").unwrap_or_else(|| format!("merge {name}")),
                    tag: extract_git_attr(l, "tag:"),
                    kind: git_commit_type(l).unwrap_or(GitCommitType::Merge),
                    parent: branches[current].head,
                    merge_parent: branches[src].head,
                    picked_from: None,
                });
                branches[current].head = Some(idx);
            }
            "cherry-pick" => {
                let Some(src_id) = extract_git_attr(l, "id:") else {
                    continue;
                };
                let Some(src) = commits.iter().position(|c| c.id == src_id) else {
                    continue;
                };
                let idx = commits.len();
                let tag = extract_git_attr(l, "tag:").unwrap_or_else(|| match extract_git_attr(l, "parent:") {
                    Some(parent) => format!("cherry-pick:{src_id}|parent:{parent}"),
                    None => format!("cherry-pick:{src_id}"),
                });
                commits.push(GitCommit {
                    branch: current,
                    id: next_id(&mut auto_idx),
                    tag: Some(tag),
                    kind: GitCommitType::CherryPick,
                    parent: branches[current].head,
                    merge_parent: None,
                    picked_from: Some(src),
                });
                branches[current].head = Some(idx);
            }
            _ => {}
        }
    }

    if commits.is_empty() {
        return None;
    }

    // Lanes: explicit `order` first by value; branches without one sit between 0 and 1 in
    // definition order (mermaid uses `0.<index>`).
    let mut lane_of_branch: Vec<usize> = (0..branches.len()).collect();
    {
        let key = |i: usize| -> f32 {
            branches[i]
                .order
                .unwrap_or_else(|| format!("0.{i}").parse::<f32>().unwrap_or(0.5))
        };
        let mut sorted: Vec<usize> = (0..branches.len()).collect();
        sorted.sort_by(|&a, &b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
        for (lane, &bi) in sorted.iter().enumerate() {
            lane_of_branch[bi] = lane;
        }
    }

    // Position along the time axis: one slot per commit, or (parallelCommits) one past the parents.
    let mut slot: Vec<usize> = Vec::with_capacity(commits.len());
    for (idx, c) in commits.iter().enumerate() {
        let s = if parallel {
            [c.parent, c.merge_parent, c.picked_from]
                .into_iter()
                .flatten()
                .map(|p| slot[p] + 1)
                .max()
                .unwrap_or(0)
        } else {
            idx
        };
        slot.push(s);
    }
    let slots = slot.iter().copied().max().unwrap_or(0) + 1;

    const COLORS: &[&str] = &["#2563eb", "#10b981", "#f59e0b", "#8b5cf6", "#ef4444", "#06b6d4", "#ec4899", "#84cc16"];
    let lane_color = |lane: usize| -> String {
        config
            .color_var(&format!("git{}", lane % 8))
            .unwrap_or_else(|| COLORS[lane % COLORS.len()].to_string())
    };
    let inv_color = |lane: usize| config.color_var(&format!("gitInv{}", lane % 8)).unwrap_or_else(|| "#ffffff".to_string());
    let px = |key: &str, default: f32| -> f32 {
        config
            .vars
            .get(key)
            .and_then(|v| v.trim().trim_end_matches("px").trim().parse::<f32>().ok())
            .filter(|v| (6.0..=40.0).contains(v))
            .unwrap_or(default)
    };
    let label_color = config.color_var("commitLabelColor").unwrap_or_else(|| "#334155".to_string());
    let label_bg = config.color_var("commitLabelBackground");
    let label_size = px("commitLabelFontSize", 10.5);
    let tag_color = config.color_var("tagLabelColor").unwrap_or_else(|| "#854d0e".to_string());
    let tag_bg = config.color_var("tagLabelBackground").unwrap_or_else(|| "#fef9c3".to_string());
    let tag_border = config.color_var("tagLabelBorder").unwrap_or_else(|| "#ca8a04".to_string());
    let tag_size = px("tagLabelFontSize", 10.0);

    let lanes = branches.len() as f32;
    let name_w = branches
        .iter()
        .map(|b| estimate_text_width(&b.name, 12.0))
        .fold(64.0f32, f32::max)
        + 28.0;
    let step = 64.0f32;
    let lane_gap = 46.0f32;
    let widest_label = commits
        .iter()
        .map(|c| estimate_text_width(&c.id, label_size))
        .fold(0.0f32, f32::max);
    let widest_tag = commits
        .iter()
        .filter_map(|c| c.tag.as_deref())
        .map(|t| estimate_text_width(t, tag_size) + 12.0)
        .fold(0.0f32, f32::max);
    // Vertical graphs put tags left of the commit, so the first lane starts after the widest tag.
    let tb_left = 24.0 + if widest_tag > 0.0 { widest_tag + 14.0 } else { 0.0 };
    let lane_w = (widest_label + 40.0).max(name_w).max(90.0);
    // (x, y) of slot `s` on lane `lane`, and the canvas size.
    let (total_w, total_h, origin) = match dir {
        GitDir::Lr => {
            let left = if show_branches { name_w } else { 20.0 };
            let bottom = if rotate_labels { 24.0 + widest_label * 0.72 } else { 36.0 };
            (left + slots as f32 * step + 36.0, 28.0 + lanes * lane_gap + bottom, left)
        }
        GitDir::Tb | GitDir::Bt => {
            let top = if show_branches { 40.0 } else { 16.0 };
            (tb_left + lanes * lane_w + 24.0, top + slots as f32 * step + 40.0, top)
        }
    };
    let pos = |s: usize, lane: usize| -> (f32, f32) {
        match dir {
            GitDir::Lr => (origin + (s as f32 + 0.5) * step, 28.0 + (lane as f32 + 0.5) * lane_gap),
            GitDir::Tb => (tb_left + (lane as f32 + 0.5) * lane_w, origin + (s as f32 + 0.5) * step),
            GitDir::Bt => (tb_left + (lane as f32 + 0.5) * lane_w, origin + ((slots - 1 - s) as f32 + 0.5) * step),
        }
    };
    let commit_pos = |idx: usize| pos(slot[idx], lane_of_branch[commits[idx].branch]);
    let total_w = total_w.ceil() as u32;
    let total_h = total_h.ceil() as u32;
    let link = |(x1, y1): (f32, f32), (x2, y2): (f32, f32)| -> String {
        if dir == GitDir::Lr {
            let mx = (x1 + x2) / 2.0;
            format!("M {x1:.1} {y1:.1} C {mx:.1} {y1:.1}, {mx:.1} {y2:.1}, {x2:.1} {y2:.1}")
        } else {
            let my = (y1 + y2) / 2.0;
            format!("M {x1:.1} {y1:.1} C {x1:.1} {my:.1}, {x2:.1} {my:.1}, {x2:.1} {y2:.1}")
        }
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));

    if show_branches {
        for (bi, branch) in branches.iter().enumerate() {
            let lane = lane_of_branch[bi];
            let color = lane_color(lane);
            let text_fill = config
                .color_var(&format!("gitBranchLabel{}", lane % 10))
                .unwrap_or_else(|| "#ffffff".to_string());
            let (first, last) = (pos(0, lane), pos(slots - 1, lane));
            let name = escape_xml(&branch.name);
            // Branch names sit in a pill of the branch colour, text in `gitBranchLabelN` (white by default).
            let pill_w = estimate_text_width(&branch.name, 12.0) + 16.0;
            let push_pill = |svg: &mut String, cx: f32, cy: f32| {
                svg.push_str(&format!(
                    r##"<rect x="{:.1}" y="{:.1}" width="{pill_w:.1}" height="20" rx="4" fill="{color}" data-role="branch-pill"/>"##,
                    cx - pill_w / 2.0,
                    cy - 10.0
                ));
                svg.push_str(&format!(
                    r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="bold" fill="{text_fill}" data-role="branch-label">{name}</text>"##,
                    cy + 4.0
                ));
            };
            match dir {
                GitDir::Lr => {
                    svg.push_str(&format!(
                        r##"<line x1="{origin:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#e2e8f0" stroke-width="1.5" stroke-dasharray="4,4"/>"##,
                        first.1,
                        total_w as f32 - 18.0,
                        first.1
                    ));
                    push_pill(&mut svg, 10.0 + (name_w - 24.0) / 2.0, first.1);
                }
                GitDir::Tb | GitDir::Bt => {
                    let (y1, y2) = (first.1.min(last.1) - step * 0.4, first.1.max(last.1) + step * 0.4);
                    svg.push_str(&format!(
                        r##"<line x1="{:.1}" y1="{y1:.1}" x2="{:.1}" y2="{y2:.1}" stroke="#e2e8f0" stroke-width="1.5" stroke-dasharray="4,4"/>"##,
                        first.0, first.0
                    ));
                    let ty = if dir == GitDir::Tb { 20.0 } else { total_h as f32 - 16.0 };
                    push_pill(&mut svg, first.0, ty);
                }
            }
        }
    }

    for (idx, c) in commits.iter().enumerate() {
        let to = commit_pos(idx);
        if let Some(p) = c.parent {
            let color = lane_color(lane_of_branch[c.branch]);
            svg.push_str(&format!(
                r##"<path d="{}" fill="none" stroke="{color}" stroke-width="2.2" data-role="edge"/>"##,
                link(commit_pos(p), to)
            ));
        }
        if let Some(mp) = c.merge_parent {
            let color = lane_color(lane_of_branch[commits[mp].branch]);
            svg.push_str(&format!(
                r##"<path d="{}" fill="none" stroke="{color}" stroke-width="2" stroke-dasharray="4,3" data-role="edge"/>"##,
                link(commit_pos(mp), to)
            ));
        }
        if let Some(src) = c.picked_from {
            svg.push_str(&format!(
                r##"<path d="{}" fill="none" stroke="#94a3b8" stroke-width="1.5" stroke-dasharray="2,3" data-link="cherry-pick"/>"##,
                link(commit_pos(src), to)
            ));
        }
    }

    for (idx, c) in commits.iter().enumerate() {
        let (cx, cy) = commit_pos(idx);
        let lane = lane_of_branch[c.branch];
        let color = lane_color(lane);
        match c.kind {
            GitCommitType::Normal => svg.push_str(&format!(
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="7.5" fill="{color}" stroke="#ffffff" stroke-width="2" data-commit-type="normal"/>"##
            )),
            GitCommitType::Merge => {
                svg.push_str(&format!(
                    r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="8.5" fill="{color}" stroke="#ffffff" stroke-width="2" data-commit-type="merge"/>"##
                ));
                svg.push_str(&format!(r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="3.5" fill="#ffffff"/>"##));
            }
            GitCommitType::Highlight => {
                svg.push_str(&format!(
                    r##"<rect x="{:.1}" y="{:.1}" width="18" height="18" fill="{color}" stroke="#ffffff" stroke-width="1.5" data-commit-type="highlight"/>"##,
                    cx - 9.0,
                    cy - 9.0
                ));
                svg.push_str(&format!(
                    r##"<rect x="{:.1}" y="{:.1}" width="10" height="10" fill="{}"/>"##,
                    cx - 5.0,
                    cy - 5.0,
                    inv_color(lane)
                ));
            }
            GitCommitType::Reverse => {
                svg.push_str(&format!(
                    r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="8.5" fill="{color}" stroke="#ffffff" stroke-width="2" data-commit-type="reverse"/>"##
                ));
                svg.push_str(&format!(
                    r##"<path d="M {:.1} {:.1} L {:.1} {:.1} M {:.1} {:.1} L {:.1} {:.1}" stroke="#ffffff" stroke-width="2" fill="none"/>"##,
                    cx - 4.0, cy - 4.0, cx + 4.0, cy + 4.0, cx + 4.0, cy - 4.0, cx - 4.0, cy + 4.0
                ));
            }
            GitCommitType::CherryPick => {
                svg.push_str(&format!(
                    r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="8.5" fill="{color}" stroke="#ffffff" stroke-width="2" data-commit-type="cherry-pick"/>"##
                ));
                for dx in [-2.6f32, 2.6] {
                    svg.push_str(&format!(r##"<circle cx="{:.1}" cy="{:.1}" r="2.2" fill="#ffffff"/>"##, cx + dx, cy + 2.0));
                }
                svg.push_str(&format!(
                    r##"<path d="M {:.1} {:.1} Q {cx:.1} {:.1} {:.1} {:.1}" stroke="#ffffff" stroke-width="1" fill="none"/>"##,
                    cx - 2.6, cy + 0.5, cy - 6.0, cx + 2.6, cy + 0.5
                ));
            }
        }
        if show_labels {
            let (lx, ly, anchor) = match dir {
                GitDir::Lr => (cx, cy + 22.0, "middle"),
                GitDir::Tb | GitDir::Bt => (cx + 14.0, cy + 4.0, "start"),
            };
            if let Some(bg) = &label_bg {
                let lw = estimate_text_width(&c.id, label_size) + 6.0;
                let bx = if anchor == "middle" { lx - lw / 2.0 } else { lx - 3.0 };
                if !rotate_labels {
                    svg.push_str(&format!(
                        r##"<rect x="{bx:.1}" y="{:.1}" width="{lw:.1}" height="{:.1}" rx="2" fill="{bg}" opacity="0.85"/>"##,
                        ly - label_size,
                        label_size + 4.0
                    ));
                }
            }
            let rotate = if rotate_labels {
                format!(r##" transform="rotate(-45 {lx:.1} {ly:.1})""##)
            } else {
                String::new()
            };
            let anchor = if rotate_labels { "end" } else { anchor };
            svg.push_str(&format!(
                r##"<text x="{lx:.1}" y="{ly:.1}" text-anchor="{anchor}" font-size="{label_size}" fill="{label_color}" data-role="commit-label"{rotate}>{}</text>"##,
                escape_xml(&c.id)
            ));
        }
        if let Some(tag) = &c.tag {
            let tw = estimate_text_width(tag, tag_size) + 12.0;
            let th = tag_size + 6.0;
            let (tx, ty) = match dir {
                GitDir::Lr => (cx - tw / 2.0, cy - 13.0 - th),
                GitDir::Tb | GitDir::Bt => (cx - 14.0 - tw, cy - th / 2.0),
            };
            svg.push_str(&format!(
                r##"<rect x="{tx:.1}" y="{ty:.1}" width="{tw:.1}" height="{th:.1}" rx="4" fill="{tag_bg}" stroke="{tag_border}" stroke-width="1" data-role="tag"/>"##
            ));
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="{tag_size}" font-weight="bold" fill="{tag_color}">{}</text>"##,
                tx + tw / 2.0,
                ty + th - 4.5,
                escape_xml(tag)
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

pub(super) fn extract_git_attr(line: &str, key: &str) -> Option<String> {
    let pos = line.find(key)?;
    let rest = line[pos + key.len()..].trim_start();
    if let Some(after_q) = rest.strip_prefix('"') {
        let end = after_q.find('"')?;
        Some(after_q[..end].to_string())
    } else {
        rest.split_whitespace().next().map(ToString::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn git_graph_commit_types_cherry_pick_order_direction_and_config() {
        let src = "gitGraph\n  commit id: \"A\"\n  commit id: \"R\" type: REVERSE\n  branch hotfix order: 3\n  branch dev order: 1\n  commit id: \"B\" type: HIGHLIGHT\n  checkout main\n  cherry-pick id: \"B\"\n  merge dev tag: \"v1\"\n";
        let svg = svg_of(src);
        for kind in ["normal", "reverse", "highlight", "cherry-pick", "merge"] {
            assert_eq!(svg.matches(&format!(r#"data-commit-type="{kind}""#)).count(), 1, "{kind}\n{svg}");
        }
        assert!(svg.contains(">cherry-pick:B<") && svg.contains(r#"data-link="cherry-pick""#));
        let merge = svg_of(
            "gitGraph
        commit id: \"ZERO\"
        branch develop
        commit id:\"B\"
        checkout main
        merge develop id:\"MERGE\"
        checkout develop
        cherry-pick id:\"MERGE\" parent:\"B\"
",
        );
        assert!(merge.contains(">cherry-pick:MERGE|parent:B<"), "{merge}");
        assert!(svg.contains(r#"data-role="edge""#), "{svg}");
        assert!(svg.contains(">v1<"));
        // `order:` sorts the lanes: main (0), dev (1), hotfix (3), whatever the definition order.
        let lane_y = |name: &str| text_y(&svg, name);
        assert!(lane_y("main") < lane_y("dev") && lane_y("dev") < lane_y("hotfix"), "{svg}");
        // Unordered branches sit between 0 and 1, in definition order.
        let svg2 = svg_of("gitGraph\n  commit\n  branch late order: 1\n  branch x\n  commit\n  branch y\n  commit\n");
        assert!(text_y(&svg2, "main") < text_y(&svg2, "x") && text_y(&svg2, "x") < text_y(&svg2, "y"));
        assert!(text_y(&svg2, "y") < text_y(&svg2, "late"));

        // Orientation: TB runs downwards, BT upwards, LR to the right.
        let flow = "  commit id: \"one\"\n  commit id: \"two\"\n";
        let lr = svg_of(&format!("gitGraph\n{flow}"));
        assert!(text_x(&lr, "two") > text_x(&lr, "one") + 30.0);
        let tb = svg_of(&format!("gitGraph TB:\n{flow}"));
        assert!(text_y(&tb, "two") > text_y(&tb, "one") + 30.0, "{tb}");
        let bt = svg_of(&format!("gitGraph BT:\n{flow}"));
        assert!(text_y(&bt, "two") + 30.0 < text_y(&bt, "one"), "{bt}");

        // Config: names, visibility, colours, rotation, parallel commits.
        let cfg = "---\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n    showBranches: false\n    showCommitLabel: false\n---\n";
        let hidden = svg_of(&format!("{cfg}gitGraph\n{flow}"));
        assert!(!hidden.contains("data-role=\"branch-label\"") && !hidden.contains("data-role=\"commit-label\""));
        assert!(!hidden.contains(">trunk<") && !hidden.contains(">main<"));
        let named = svg_of("---\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n    rotateCommitLabel: true\n  themeVariables:\n    git0: '#123456'\n    gitBranchLabel0: '#fedcba'\n    commitLabelColor: '#abcdef'\n---\ngitGraph\n  commit id: \"one\"\n");
        assert!(named.contains(">trunk<") && named.contains(r##"fill="#123456""##) && named.contains(r##"fill="#fedcba""##));
        assert!(named.contains(r##"fill="#abcdef""##) && named.contains("rotate(-45"));
        let branchy = "gitGraph\n  commit\n  branch a\n  commit\n  commit\n  checkout main\n  commit\n  commit\n";
        let serial = render_mermaid(branchy);
        let parallel = render_mermaid(&format!("---\nconfig:\n  gitGraph:\n    parallelCommits: true\n---\n{branchy}"));
        let width = |out: Option<MermaidOutput>| match out {
            Some(MermaidOutput::Svg { width_px, .. }) => width_px,
            _ => panic!("svg"),
        };
        assert!(width(parallel) + 100 < width(serial));
    }
}
