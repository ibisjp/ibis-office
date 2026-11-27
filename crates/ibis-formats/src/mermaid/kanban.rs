// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `kanban`: columns side by side, each with its cards stacked under the title.

use super::*;

#[derive(Debug, Default, PartialEq)]
pub(super) struct KanbanCard {
    pub(super) text: String,
    pub(super) ticket: Option<String>,
    pub(super) assigned: Option<String>,
    pub(super) priority: Option<String>,
}

pub(super) struct KanbanColumn {
    pub(super) title: String,
    pub(super) cards: Vec<KanbanCard>,
}

/// `id[Text]`, `[Text]` or `Text` → the text.
pub(super) fn kanban_text(node: &str) -> String {
    let node = node.trim();
    match (node.find('['), node.ends_with(']')) {
        (Some(open), true) if !node[..open].contains(char::is_whitespace) => unquote(node[open + 1..node.len() - 1].trim()),
        _ => unquote(node),
    }
}

/// `@{ ticket: MC-2037, assigned: 'knsv', priority: 'High' }` → (key, value) pairs.
pub(super) fn kanban_metadata(meta: &str) -> Vec<(String, String)> {
    let inner = meta.trim().trim_start_matches('{').trim_end_matches('}');
    let mut out = Vec::new();
    let mut quote: Option<char> = None;
    let mut part = String::new();
    let mut parts = Vec::new();
    for c in inner.chars() {
        match (c, quote) {
            ('\'' | '"', None) => quote = Some(c),
            (q, Some(open)) if q == open => quote = None,
            (',', None) => {
                parts.push(std::mem::take(&mut part));
                continue;
            }
            _ => {}
        }
        part.push(c);
    }
    parts.push(part);
    for p in parts {
        if let Some((k, v)) = p.split_once(':') {
            out.push((k.trim().to_string(), unquote(v.trim())));
        }
    }
    out
}

pub(super) fn parse_kanban(source: &str) -> Vec<KanbanColumn> {
    let mut columns: Vec<KanbanColumn> = Vec::new();
    let mut column_indent: Option<usize> = None;
    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let (node, meta) = match l.find("@{") {
            Some(at) => (&l[..at], Some(&l[at + 1..])),
            None => (l, None),
        };
        let is_column = match column_indent {
            None => true,
            Some(ci) => indent <= ci,
        };
        if is_column || columns.is_empty() {
            column_indent = Some(column_indent.map_or(indent, |ci| ci.min(indent)));
            columns.push(KanbanColumn { title: kanban_text(node), cards: Vec::new() });
            continue;
        }
        let mut card = KanbanCard { text: kanban_text(node), ..KanbanCard::default() };
        for (key, value) in meta.map(kanban_metadata).unwrap_or_default() {
            match key.as_str() {
                "ticket" => card.ticket = Some(value),
                "assigned" => card.assigned = Some(value),
                "priority" => card.priority = Some(value),
                _ => {}
            }
        }
        columns.last_mut().unwrap().cards.push(card);
    }
    columns
}

/// The stripe colour of a card's priority, like Mermaid's.
fn priority_color(priority: &str) -> Option<&'static str> {
    match priority.to_ascii_lowercase().as_str() {
        "very high" => Some("#dc2626"),
        "high" => Some("#f97316"),
        "low" => Some("#3b82f6"),
        "very low" => Some("#93c5fd"),
        _ => None,
    }
}

pub(super) fn render_kanban_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let columns = parse_kanban(source);
    if columns.is_empty() {
        return None;
    }
    // `kanban.sectionWidth` (Mermaid's default is 200).
    let col_w = config
        .option("kanban.sectionWidth")
        .and_then(|v| px_number(v))
        .map_or(200.0, |v| (v as f32).clamp(100.0, 600.0));
    const GAP: f32 = 12.0;
    const PAD: f32 = 10.0;
    const FONT: f32 = 12.0;
    const LINE: f32 = 15.0;
    const HEADER: f32 = 34.0;
    const COLORS: &[&str] = &["#dbeafe", "#dcfce7", "#fef3c7", "#fce7f3", "#ede9fe", "#cffafe"];
    let text_w = col_w - 2.0 * PAD - 22.0;

    // Card heights: wrapped text, plus a row for the ticket / assignee.
    let card_lines: Vec<Vec<Vec<String>>> =
        columns.iter().map(|c| c.cards.iter().map(|card| label_lines(&card.text, text_w, FONT)).collect()).collect();
    let card_h = |card: &KanbanCard, lines: &[String]| {
        lines.len() as f32 * LINE + 16.0 + if card.ticket.is_some() || card.assigned.is_some() { 16.0 } else { 0.0 }
    };
    let col_h: Vec<f32> = columns
        .iter()
        .zip(&card_lines)
        .map(|(c, lines)| HEADER + c.cards.iter().zip(lines).map(|(card, l)| card_h(card, l) + 8.0).sum::<f32>() + PAD)
        .collect();
    let body_h = col_h.iter().copied().fold(HEADER + 40.0, f32::max);
    let total_w = (columns.len() as f32 * (col_w + GAP) - GAP + 2.0 * 16.0).ceil() as u32;
    let total_h = (body_h + 32.0).ceil() as u32;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    for (ci, column) in columns.iter().enumerate() {
        let x = 16.0 + ci as f32 * (col_w + GAP);
        let y = 16.0;
        let bg = config
            .color_var(&format!("cScale{}", ci % 12))
            .unwrap_or_else(|| COLORS[ci % COLORS.len()].to_string());
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{col_w:.1}" height="{body_h:.1}" rx="6" fill="{bg}" fill-opacity="0.55" stroke="#cbd5e1" stroke-width="1" data-role="kanban-column"/>"##
        ));
        let title = label_lines(&column.title, col_w - 16.0, 13.0).into_iter().next().unwrap_or_default();
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="bold" fill="#0f172a">{}</text>"##,
            x + col_w / 2.0,
            y + 22.0,
            escape_xml(&title)
        ));
        let mut cy = y + HEADER;
        for (card, lines) in column.cards.iter().zip(&card_lines[ci]) {
            let h = card_h(card, lines);
            let (cx, cw) = (x + PAD, col_w - 2.0 * PAD);
            svg.push_str(&format!(
                r##"<rect x="{cx:.1}" y="{cy:.1}" width="{cw:.1}" height="{h:.1}" rx="4" fill="#ffffff" stroke="#94a3b8" stroke-width="1" data-role="kanban-card"/>"##
            ));
            if let Some(color) = card.priority.as_deref().and_then(priority_color) {
                svg.push_str(&format!(
                    r##"<rect x="{cx:.1}" y="{cy:.1}" width="4" height="{h:.1}" rx="2" fill="{color}" data-priority="{}"/>"##,
                    escape_attr(card.priority.as_deref().unwrap_or(""))
                ));
            }
            for (k, line) in lines.iter().enumerate() {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="{FONT}" fill="#0f172a">{}</text>"##,
                    cx + 12.0,
                    cy + 18.0 + k as f32 * LINE,
                    escape_xml(line)
                ));
            }
            let meta_y = cy + h - 8.0;
            if let Some(ticket) = &card.ticket {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{meta_y:.1}" font-size="10" fill="#2563eb" data-role="ticket">{}</text>"##,
                    cx + 12.0,
                    escape_xml(ticket)
                ));
            }
            if let Some(who) = &card.assigned {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{meta_y:.1}" text-anchor="end" font-size="10" fill="#475569" data-role="assigned">{}</text>"##,
                    cx + cw - 8.0,
                    escape_xml(who)
                ));
            }
            cy += h + 8.0;
        }
    }
    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn kanban_columns_cards_and_metadata() {
        let src = "kanban
  Todo
    [Create Documentation]
    docs[Create Blog about the new diagram]
  [In progress]
    id6[Create renderer so that it works in all cases. We also add some extra text here for testing purposes.]
  id10[Ready for test]
    id4[Create parsing tests]@{ ticket: MC-2038, assigned: 'K.Sveidqvist', priority: 'High' }
    id66[last item]@{ priority: 'Very Low', assigned: 'knsv' }
";
        let columns = parse_kanban(src);
        assert_eq!(columns.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), vec!["Todo", "In progress", "Ready for test"]);
        assert_eq!(columns.iter().map(|c| c.cards.len()).collect::<Vec<_>>(), vec![2, 1, 2]);
        assert_eq!(
            columns[2].cards[0],
            KanbanCard {
                text: "Create parsing tests".into(),
                ticket: Some("MC-2038".into()),
                assigned: Some("K.Sveidqvist".into()),
                priority: Some("High".into()),
            }
        );
        let svg = svg_of(src);
        assert_eq!(svg.matches(r#"data-role="kanban-column""#).count(), 3);
        assert_eq!(svg.matches(r#"data-role="kanban-card""#).count(), 5);
        assert!(svg.contains(r#"data-priority="High""#) && svg.contains(r#"data-priority="Very Low""#));
        assert!(svg.contains(">MC-2038<") && svg.contains(">K.Sveidqvist<"));
        // The long card wraps inside its column.
        assert!(tag_with(&svg, r##"font-size="12" fill="#0f172a">"##).len() > 5, "{svg}");
        let columns_x: Vec<f32> = tag_with(&svg, r#"data-role="kanban-column""#).iter().map(|t| attr_f32(t, "x").unwrap()).collect();
        assert!(columns_x.windows(2).all(|w| w[1] > w[0] + 200.0), "{columns_x:?}");
    }

    #[test]
    fn kanban_metadata_quotes_and_bare_values() {
        assert_eq!(
            kanban_metadata("{ ticket: MC-2037, assigned: knsv, priority: 'Very, High' }"),
            vec![("ticket".into(), "MC-2037".into()), ("assigned".into(), "knsv".into()), ("priority".into(), "Very, High".into())]
        );
        assert_eq!(kanban_text("id12[Can't reproduce]"), "Can't reproduce");
        assert_eq!(kanban_text("[In progress]"), "In progress");
        assert_eq!(kanban_text("Todo"), "Todo");
        // A single unindented column with an indented card.
        let one = parse_kanban("kanban\ntodo[Todo]\n  id3[Update Database Function]@{ ticket: MC-2037 }\n");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].cards[0].ticket.as_deref(), Some("MC-2037"));
    }
}
