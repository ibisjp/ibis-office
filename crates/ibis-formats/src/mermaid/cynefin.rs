// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use super::*;

const DOMAINS: [(&str, &str, &str, &str); 5] = [
    ("complex", "Complex", "#dbeafe", "Probe → Sense → Respond"),
    (
        "complicated",
        "Complicated",
        "#dcfce7",
        "Sense → Analyse → Respond",
    ),
    ("chaotic", "Chaotic", "#fee2e2", "Act → Sense → Respond"),
    ("clear", "Clear", "#fef3c7", "Sense → Categorise → Respond"),
    ("confusion", "Confusion", "#f1f5f9", "Clarify the domain"),
];

fn domain(name: &str) -> Option<usize> {
    DOMAINS.iter().position(|d| d.0 == name.trim())
}

pub(super) fn render_cynefin_svg(
    source: &str,
    config: &MermaidConfig,
) -> Option<(Vec<u8>, u32, u32)> {
    let mut items: [Vec<String>; 5] = std::array::from_fn(|_| Vec::new());
    let mut transitions = Vec::new();
    let mut current: Option<usize> = None;
    let mut title = None;
    for line in source.lines().skip(1).map(str::trim) {
        if let Some(value) = line.strip_prefix("title ") {
            title = Some(unquote(value));
        } else if let Some(index) = domain(line) {
            current = Some(index);
        } else if line.starts_with('"') && line.ends_with('"') && line.len() >= 2 {
            if let Some(index) = current {
                items[index].push(unquote(line));
            }
        } else if let Some((from, rest)) = line.split_once("-->") {
            let (to, label) = rest.split_once(':').unwrap_or((rest, ""));
            if let (Some(from), Some(to)) = (domain(from), domain(to))
                && from != to
            {
                transitions.push((from, to, unquote(label)));
            }
        }
    }

    let number = |key, default, max| {
        config
            .option(key)
            .and_then(px_number)
            .map_or(default, |value| value.min(max) as f32)
    };
    let padding = number("cynefin.padding", 40.0, 160.0);
    let width = number("cynefin.width", 800.0, 4000.0)
        .max(440.0 + padding * 2.0)
        .ceil() as u32;
    let half = width as f32 / 2.0;
    let amplitude = number("cynefin.boundaryAmplitude", 8.0, 20.0);
    let descriptions = config.flag("cynefin.showDomainDescriptions", true);
    let column = half - padding;
    let item_width = column - 56.0;
    let heading = if descriptions { 62.0 } else { 42.0 };
    let rows = |text: &str, max| label_lines(text, max, 13.0).len() as f32 * 17.0 + 8.0;
    let domain_height = |index: usize| {
        heading
            + 16.0
            + items[index]
                .iter()
                .map(|item| rows(item, item_width))
                .sum::<f32>()
    };
    let center_items = items[4]
        .iter()
        .take(3)
        .cloned()
        .chain((items[4].len() > 3).then(|| format!("+{} more", items[4].len() - 3)))
        .collect::<Vec<_>>();
    let center_height = heading
        + 28.0
        + center_items
            .iter()
            .map(|item| rows(item, item_width))
            .sum::<f32>();
    let transition_width = width as f32 - padding * 2.0 - 16.0;
    let title_height = title.as_deref().map_or(0.0, |label| {
        label_lines(label, transition_width, 18.0).len() as f32 * 23.0 + 12.0
    });
    let top = padding + title_height;
    let min_height = number("cynefin.height", 600.0, 4000.0);
    let row_min = ((min_height - top - center_height - padding) / 2.0).max(100.0);
    let upper = domain_height(0).max(domain_height(1)).max(row_min);
    let lower = domain_height(2).max(domain_height(3)).max(row_min);
    let center_top = top + upper;
    let bottom = center_top + center_height + lower;
    let height = (bottom
        + padding
        + transitions
            .iter()
            .map(|(_, _, label)| {
                if label.is_empty() {
                    25.0
                } else {
                    25.0 + rows(label, transition_width)
                }
            })
            .sum::<f32>())
    .ceil() as u32;
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="sans-serif"><rect width="{width}" height="{height}" fill="#ffffff"/>"##
    );
    if let Some(title) = title {
        push_centered_label(
            &mut svg,
            half,
            padding + (title_height - 12.0) / 2.0,
            &title,
            18.0,
            transition_width,
            "#0f172a",
            "bold",
        );
    }
    for (index, &(key, name, color, subtitle)) in DOMAINS.iter().enumerate() {
        let (x, y, h) = match index {
            0 => (padding, top, upper + center_height / 2.0),
            1 => (half, top, upper + center_height / 2.0),
            2 => (
                padding,
                center_top + center_height / 2.0,
                lower + center_height / 2.0,
            ),
            3 => (
                half,
                center_top + center_height / 2.0,
                lower + center_height / 2.0,
            ),
            _ => (half - column / 2.0, center_top, center_height),
        };
        let cx = x + column / 2.0;
        svg.push_str(&format!(r#"<g data-domain="{key}">"#));
        if index == 4 {
            let split = y + h / 2.0;
            let right = width as f32 - padding;
            svg.push_str(&format!(
                r##"<path d="M {half} {top} C {} {}, {} {}, {half} {split} S {} {}, {half} {bottom}" fill="none" stroke="#64748b" stroke-width="2"/><path d="M {padding} {split} H {half}" stroke="#64748b" stroke-dasharray="5 4"/><path d="M {half} {split} H {right}" stroke="#b45309" stroke-width="3" data-role="cliff"/><rect x="{x}" y="{y}" width="{column}" height="{h}" rx="26" fill="{color}" stroke="#64748b"/>"##,
                half + amplitude, top + upper / 2.0, half - amplitude, split - upper / 3.0,
                half + amplitude, bottom - lower / 3.0
            ));
        } else {
            svg.push_str(&format!(
                r#"<rect x="{x}" y="{y}" width="{column}" height="{h}" fill="{color}"/>"#
            ));
        }
        let text_y = if matches!(index, 2 | 3) {
            y + center_height / 2.0
        } else {
            y
        };
        push_centered_label(
            &mut svg,
            cx,
            text_y + 22.0,
            name,
            17.0,
            item_width,
            "#0f172a",
            "bold",
        );
        if descriptions {
            push_centered_label(
                &mut svg,
                cx,
                text_y + 44.0,
                subtitle,
                12.0,
                item_width,
                "#334155",
                "normal",
            );
        }
        let mut at = text_y + heading;
        let labels = if index == 4 {
            &center_items
        } else {
            &items[index]
        };
        for label in labels {
            let h = rows(label, item_width);
            push_centered_label(
                &mut svg,
                cx,
                at + h / 2.0,
                label,
                13.0,
                item_width,
                "#0f172a",
                "normal",
            );
            at += h;
        }
        svg.push_str("</g>");
    }
    let mut y = bottom + 17.0;
    for (from, to, label) in transitions {
        let direction = format!("{} → {}", DOMAINS[from].1, DOMAINS[to].1);
        svg.push_str(r#"<g data-role="transition">"#);
        push_centered_label(
            &mut svg,
            half,
            y,
            &direction,
            13.0,
            transition_width,
            "#1e3a8a",
            "bold",
        );
        y += 25.0;
        if !label.is_empty() {
            let h = rows(&label, transition_width);
            push_centered_label(
                &mut svg,
                half,
                y + h / 2.0 - 12.0,
                &label,
                13.0,
                transition_width,
                "#0f172a",
                "normal",
            );
            y += h;
        }
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
    Some((svg.into_bytes(), width, height))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;

    #[test]
    fn cynefin_domains_and_transitions() {
        let svg = svg_of(
            "cynefin-beta\n title 判断\n clear\n \"既知 & 対処\"\n confusion\n \"不明\"\n clear --> chaotic : \"悪化\"\n complex --> complex\n",
        );
        assert!(svg.contains("既知 &amp; 対処") && svg.contains(">不明<"));
        assert!(svg.contains(">Clear → Chaotic<") && svg.contains(">悪化<"));
        assert_eq!(svg.matches("data-role=\"transition\"").count(), 1);
        assert_eq!(svg.matches("data-domain=").count(), 5);
    }
}
