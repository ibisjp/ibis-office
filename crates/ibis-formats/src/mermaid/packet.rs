// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `packet` / `packet-beta`: bit fields laid out in rows of `bitsPerRow` bits.

use super::*;

#[derive(Debug, PartialEq)]
pub(super) struct PacketField {
    pub(super) start: u32,
    /// Inclusive.
    pub(super) end: u32,
    pub(super) label: String,
}

/// Fields and the title. `0-15: "x"`, `106: "x"`, `+16: "x"` (after the previous field).
pub(super) fn parse_packet(source: &str) -> (Option<String>, Vec<PacketField>) {
    let mut title = None;
    let mut fields: Vec<PacketField> = Vec::new();
    for raw in source.lines().skip(1) {
        let l = raw.trim();
        let l = l.split_once("%%").map_or(l, |(code, _)| code.trim());
        if l.is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            title = Some(unquote(rest.trim()));
            continue;
        }
        let Some((range, label)) = l.split_once(':') else { continue };
        let range = range.trim();
        let next = fields.last().map_or(0, |f| f.end + 1);
        let (start, end) = if let Some(count) = range.strip_prefix('+') {
            match count.trim().parse::<u32>() {
                Ok(n) if n > 0 => (next, next + n - 1),
                _ => continue,
            }
        } else if let Some((a, b)) = range.split_once('-') {
            match (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                (Ok(a), Ok(b)) if b >= a => (a, b),
                _ => continue,
            }
        } else {
            match range.parse::<u32>() {
                Ok(a) => (a, a),
                Err(_) => continue,
            }
        };
        // Keep the drawing bounded: at most 64 Kibit.
        if end >= 65_536 {
            continue;
        }
        fields.push(PacketField { start, end, label: unquote(label.trim()) });
    }
    (title, fields)
}

pub(super) fn render_packet_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let (title, fields) = parse_packet(source);
    if fields.is_empty() {
        return None;
    }
    let number = |key: &str, default: f32, lo: f32, hi: f32| {
        config.option(key).and_then(|v| px_number(v)).map_or(default, |v| (v as f32).clamp(lo, hi))
    };
    let bits_per_row = number("packet.bitsPerRow", 32.0, 1.0, 256.0) as u32;
    let bit_w = number("packet.bitWidth", 32.0, 4.0, 128.0);
    let row_h = number("packet.rowHeight", 32.0, 12.0, 200.0);
    let pad_x = number("packet.paddingX", 5.0, 0.0, 40.0);
    let pad_y = number("packet.paddingY", 5.0, 0.0, 40.0);
    let show_bits = config.flag("packet.showBits", true);
    let descending = config.option("packet.bitOrder").is_some_and(|v| unquote(v).eq_ignore_ascii_case("descending"));
    let var = |key: &str, fallback: &str| config.vars.get(&format!("packet.{key}")).and_then(|v| css_color(v)).unwrap_or_else(|| fallback.to_string());
    let fill = var("blockFillColor", "#eff6ff");
    let stroke = var("blockStrokeColor", "#1e3a8a");
    let ink = var("labelColor", "#0f172a");
    let start_ink = var("startByteColor", "#64748b");
    let end_ink = var("endByteColor", "#64748b");
    let title_ink = var("titleColor", "#0f172a");

    let last_bit = fields.iter().map(|f| f.end).max().unwrap_or(0);
    let rows = last_bit / bits_per_row + 1;
    let bits_h = if show_bits { 14.0 } else { 0.0 };
    let line_h = row_h + bits_h + pad_y;
    let top = if title.is_some() { 40.0 } else { 12.0 };
    let total_w = (bits_per_row as f32 * bit_w + 2.0 * 12.0).ceil() as u32;
    let total_h = (top + rows as f32 * line_h + 12.0).ceil() as u32;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    if let Some(t) = &title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="26" text-anchor="middle" font-size="15" font-weight="bold" fill="{title_ink}">{}</text>"##,
            total_w as f32 / 2.0,
            escape_xml(t)
        ));
    }
    for field in &fields {
        // A field that crosses a row boundary is drawn as one piece per row.
        let mut start = field.start;
        while start <= field.end {
            let row = start / bits_per_row;
            let end = field.end.min((row + 1) * bits_per_row - 1);
            let bits = end - start + 1;
            let first_col = start % bits_per_row;
            let col = if descending { bits_per_row - first_col - bits } else { first_col };
            let x = 12.0 + col as f32 * bit_w;
            let y = top + row as f32 * line_h + bits_h;
            let w = (bits as f32 * bit_w - pad_x).max(2.0);
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{row_h:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1" data-bits="{start}-{end}"/>"##
            ));
            let label = label_lines(&field.label, w - 4.0, 11.0).into_iter().next().unwrap_or_default();
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="{ink}">{}</text>"##,
                x + w / 2.0,
                y + row_h / 2.0 + 4.0,
                escape_xml(&label)
            ));
            if show_bits {
                let by = y - 3.0;
                let (left, right) = if descending { (end, start) } else { (start, end) };
                if bits == 1 {
                    svg.push_str(&format!(
                        r##"<text x="{:.1}" y="{by:.1}" text-anchor="middle" font-size="9" fill="{start_ink}" data-role="bit">{start}</text>"##,
                        x + w / 2.0
                    ));
                } else {
                    svg.push_str(&format!(
                        r##"<text x="{x:.1}" y="{by:.1}" font-size="9" fill="{start_ink}" data-role="bit">{left}</text><text x="{:.1}" y="{by:.1}" text-anchor="end" font-size="9" fill="{end_ink}" data-role="bit">{right}</text>"##,
                        x + w
                    ));
                }
            }
            start = end + 1;
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
    fn packet_ranges_counts_and_single_bits() {
        let (title, fields) = parse_packet("packet\ntitle UDP Packet\n+16: \"Source Port\"\n+16: \"Destination Port\"\n32-47: \"Length\"\n106: \"URG\" %% one bit\n");
        assert_eq!(title.as_deref(), Some("UDP Packet"));
        assert_eq!(
            fields.iter().map(|f| (f.start, f.end, f.label.as_str())).collect::<Vec<_>>(),
            vec![(0, 15, "Source Port"), (16, 31, "Destination Port"), (32, 47, "Length"), (106, 106, "URG")]
        );
    }

    #[test]
    fn packet_rows_wrap_and_split_fields() {
        let svg = svg_of("packet\n0-15: \"A\"\n16-47: \"B\"\n");
        // 32 bits per row: B is split over two rows.
        let blocks = tag_with(&svg, "data-bits=");
        assert_eq!(blocks.len(), 3, "{svg}");
        assert!(blocks[1].contains(r#"data-bits="16-31""#) && blocks[2].contains(r#"data-bits="32-47""#));
        let y = |t: &str| attr_f32(t, "y").unwrap();
        assert!(y(blocks[2]) > y(blocks[1]));
        assert_eq!(view_box(&svg).0, 32.0 * 32.0 + 24.0);
        assert!(svg.contains(r#"data-role="bit">0<"#) && svg.contains(r#"data-role="bit">47<"#));
    }

    #[test]
    fn packet_config_bits_per_row_order_and_colours() {
        let src = "---\nconfig:\n  packet:\n    showBits: true\n    bitOrder: descending\n    bitsPerRow: 16\n  themeVariables:\n    packet:\n      startByteColor: red\n---\npacket\n0-7: \"DATA\"\n8-11: \"TYPE\"\n12: \"EN\"\n13-15: \"RESERVED\"\n";
        let svg = svg_of(src);
        assert_eq!(view_box(&svg).0, 16.0 * 32.0 + 24.0);
        let x_of = |bits: &str| attr_f32(tag_with(&svg, &format!(r#"data-bits="{bits}""#))[0], "x").unwrap();
        // Descending: bit 0 is on the right.
        assert!(x_of("0-7") > x_of("13-15"), "{svg}");
        assert!(svg.contains(r#"fill="red" data-role="bit">7<"#), "{svg}");
        let hidden = svg_of("---\nconfig:\n  packet:\n    showBits: false\n---\npacket\n0-7: \"A\"\n");
        assert!(!hidden.contains("data-role=\"bit\""));
    }
}
