// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Ishikawa cause-and-effect diagrams.

use super::*;

const FONT: f32 = 13.0;
const LABEL_WIDTH: f32 = 196.0;
const LINE_HEIGHT: f32 = 17.0;
const DEPTH_INDENT: f32 = 16.0;
const DIAGONAL: f32 = 44.0;

struct Cause {
    label: String,
    parent: Option<usize>,
    depth: usize,
    height: f32,
    width: f32,
}

fn parse(source: &str) -> Option<(String, Vec<Vec<Cause>>)> {
    let mut description = false;
    let mut lines = source.lines().skip(1).filter_map(|line| {
        let label = line.trim();
        if description {
            description = !label.starts_with('}');
            return None;
        }
        if label.starts_with("accDescr") && label.contains('{') {
            description = !label.contains('}');
            return None;
        }
        if label.is_empty()
            || label.starts_with("%%")
            || label.starts_with("accTitle:")
            || label.starts_with("accTitle ")
            || label.starts_with("accDescr:")
        {
            return None;
        }
        let indent = line
            .chars()
            .take_while(|c| matches!(c, ' ' | '\t'))
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum::<usize>();
        Some((indent, unquote(label)))
    });

    let (_, effect) = lines.next()?;
    if effect.is_empty() {
        return None;
    }

    let mut branches: Vec<Vec<Cause>> = Vec::new();
    let mut ancestors: Vec<(usize, usize)> = Vec::new();
    let mut base_indent = None;
    for (indent, label) in lines {
        let indent = indent.saturating_sub(*base_indent.get_or_insert(indent));
        while ancestors.last().is_some_and(|&(level, _)| indent <= level) {
            ancestors.pop();
        }
        if ancestors.is_empty() {
            branches.push(Vec::new());
        }
        let branch = branches.last_mut()?;
        let parent = ancestors.last().map(|&(_, row)| row);
        let wrapped = label_lines(&label, LABEL_WIDTH, FONT);
        let width = wrapped
            .iter()
            .map(|line| estimate_text_width(line, FONT))
            .fold(0.0, f32::max);
        let height = wrapped.len() as f32 * LINE_HEIGHT;
        let row = branch.len();
        branch.push(Cause {
            label,
            parent,
            depth: ancestors.len(),
            height,
            width,
        });
        ancestors.push((indent, row));
    }
    Some((effect, branches))
}

pub(super) fn render_ishikawa_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let (effect, branches) = parse(source)?;
    let effect_lines = label_lines(&effect, LABEL_WIDTH, 15.0);
    let head_width = effect_lines
        .iter()
        .map(|line| estimate_text_width(line, 15.0))
        .fold(0.0, f32::max)
        .max(70.0)
        + 38.0;
    let head_half = (effect_lines.len() as f32 * 19.0 + 22.0) / 2.0;
    let branch_height =
        |branch: &[Cause]| branch.iter().map(|cause| cause.height + 11.0).sum::<f32>() + 30.0;
    let side_height = |parity| {
        branches
            .iter()
            .skip(parity)
            .step_by(2)
            .map(|branch| branch_height(branch))
            .fold(head_half, f32::max)
    };
    let spine_y = side_height(0) + 18.0;
    let height = (spine_y + side_height(1) + 18.0).ceil() as u32;

    let mut svg = String::new();
    let mut left = 20.0;
    for pair in branches.chunks(2) {
        let labels_width = pair
            .iter()
            .flatten()
            .map(|cause| cause.depth as f32 * DEPTH_INDENT + cause.width)
            .fold(0.0, f32::max);
        let tip_x = left + labels_width + 19.0;
        let joint_x = tip_x + DIAGONAL;
        for (side, branch) in pair.iter().enumerate() {
            let outer_y = spine_y + if side == 0 { -1.0 } else { 1.0 } * branch_height(branch);
            svg.push_str(&format!(
                r##"<path d="M {tip_x:.1} {outer_y:.1} L {joint_x:.1} {spine_y:.1}" stroke="#475569" fill="none" data-role="cause-branch"/>"##
            ));
            // Keep row baselines in reading order on both sides of the spine.
            let mut row_top = if side == 0 { outer_y } else { spine_y + 22.0 };
            let mut rows: Vec<(f32, f32)> = Vec::with_capacity(branch.len());
            for cause in branch {
                let x = left + cause.depth as f32 * DEPTH_INDENT;
                let y = row_top + cause.height;
                let rib_x = tip_x + (joint_x - tip_x) * (y - outer_y) / (spine_y - outer_y);
                let path =
                    if let Some(parent) = cause.parent.filter(|&parent| branch[parent].depth > 0) {
                        let (px, py) = rows[parent];
                        format!("M {px:.1} {py:.1} V {y:.1} H {:.1}", x + cause.width + 3.0)
                    } else {
                        format!("M {x:.1} {y:.1} H {rib_x:.1}")
                    };
                svg.push_str(&format!(
                    r##"<path d="{path}" stroke="#94a3b8" fill="none" data-depth="{}"/>"##,
                    cause.depth
                ));
                let weight = if cause.depth == 0 { "bold" } else { "normal" };
                push_centered_label(
                    &mut svg,
                    x + cause.width / 2.0,
                    row_top + cause.height / 2.0 - 2.0,
                    &cause.label,
                    FONT,
                    LABEL_WIDTH,
                    "#0f172a",
                    weight,
                );
                rows.push((x, y));
                row_top += cause.height + 11.0;
            }
        }
        left = joint_x + 24.0;
    }

    let head_left = left + 18.0;
    let head_right = head_left + head_width;
    let width = (head_right + 18.0).ceil() as u32;
    svg.push_str(&format!(
        r##"<path d="M 20 {spine_y:.1} H {head_left:.1}" stroke="#475569" stroke-width="2" data-role="spine"/><path d="M {head_left:.1} {:.1} H {:.1} L {head_right:.1} {spine_y:.1} L {:.1} {:.1} H {head_left:.1} Z" fill="#dbeafe" stroke="#2563eb" data-role="effect"/>"##,
        spine_y - head_half, head_right - 18.0, head_right - 18.0, spine_y + head_half
    ));
    push_centered_label(
        &mut svg,
        (head_left + head_right - 12.0) / 2.0,
        spine_y,
        &effect,
        15.0,
        LABEL_WIDTH,
        "#0f172a",
        "bold",
    );
    Some((format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="sans-serif"><rect width="{width}" height="{height}" fill="#ffffff"/>{svg}</svg>"##
    ).into_bytes(), width, height))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    #[test]
    fn ishikawa_nested_causes_and_accessibility() {
        let source = "ishikawa-beta\n  結果 & 対策\n  accTitle: 特性要因図\n  %% comment\n  人\n    練習\n      引継ぎ\n  設備\n    故障\n";
        let (_, branches) = parse(source).unwrap();
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0][2].parent, Some(1));
        let svg = svg_of(source);
        assert!(svg.contains("<title>特性要因図</title>"));
        assert!(svg.contains("結果 &amp; 対策"));
        assert_eq!(svg.matches("data-role=\"cause-branch\"").count(), 2);
        assert!(text_y(&svg, "人") < text_y(&svg, "設備"));
        assert!(text_x(&svg, "引継ぎ") > text_x(&svg, "練習"));
        let (_, branches) =
            parse("ishikawa-beta\nEffect\n  First\nSecond\n  Third\n    Detail").unwrap();
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[2][1].parent, Some(0));
        assert!(render_mermaid("ishikawa-beta\n%% empty").is_none());
    }
}
