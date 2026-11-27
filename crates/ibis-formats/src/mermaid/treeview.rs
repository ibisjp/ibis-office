// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `treeView-beta`: file trees written with box-drawing prefixes (`├──`, `└──`, `│`) or plain
//! indentation. Entries take `:::class`, `icon(...)` and `## comment`; connector lines,
//! folder / file glyphs (`showIcons`), `rowIndent`, `lineThickness` and the `treeView` theme
//! variables follow the config.

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TreeEntry {
    pub(super) name: String,
    pub(super) depth: usize,
    pub(super) classes: Vec<String>,
    /// `icon(logos:react)`; `Some("none")` hides the glyph.
    pub(super) icon: Option<String>,
    pub(super) comment: Option<String>,
}

fn is_tree_prefix(c: char) -> bool {
    matches!(c, '│' | '├' | '└' | '─' | '|' | '`' | ' ' | '\t' | '\u{a0}')
}

/// `App.tsx :::highlight icon(logos:react) ## main component`.
pub(super) fn parse_tree_item(text: &str) -> (String, Vec<String>, Option<String>, Option<String>) {
    let (body, comment) = match text.find(" ##").or_else(|| text.strip_prefix("##").map(|_| 0)) {
        Some(at) => (&text[..at], Some(text[at..].trim_start().trim_start_matches('#').trim().to_string()).filter(|c| !c.is_empty())),
        None => (text, None),
    };
    let mut name_parts = Vec::new();
    let mut classes = Vec::new();
    let mut icon = None;
    for word in body.split_whitespace() {
        if let Some(c) = word.strip_prefix(":::") {
            classes.extend(c.split(',').filter(|c| !c.is_empty()).map(str::to_string));
        } else if let Some(i) = word.strip_prefix("icon(").and_then(|w| w.strip_suffix(')')) {
            icon = Some(i.to_string());
        } else {
            name_parts.push(word);
        }
    }
    (unquote(&name_parts.join(" ")), classes, icon, comment)
}

pub(super) fn parse_tree(source: &str) -> Vec<TreeEntry> {
    let mut out = Vec::new();
    // Prefix widths of the open ancestors; an entry is one deeper than every narrower prefix.
    let mut stack: Vec<usize> = Vec::new();
    for raw in source.lines().skip(1) {
        let line = raw.trim_end();
        let indent = line.chars().take_while(|c| is_tree_prefix(*c)).count();
        let text: String = line.chars().skip(indent).collect();
        if text.trim().is_empty() || text.starts_with("%%") || text.starts_with("accTitle") || text.starts_with("accDescr") {
            continue;
        }
        while stack.last().is_some_and(|&w| w >= indent) {
            stack.pop();
        }
        let depth = stack.len();
        stack.push(indent);
        let (name, classes, icon, comment) = parse_tree_item(text.trim());
        out.push(TreeEntry { name, depth, classes, icon, comment });
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Folder,
    File,
    /// A named icon from a pack that is not bundled: drawn as mermaid.js draws an
    /// unregistered icon, a `?`.
    Unknown,
}

fn push_glyph(svg: &mut String, glyph: Glyph, x: f32, cy: f32, size: f32) {
    let s = size / 14.0;
    if glyph == Glyph::Unknown {
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="2" fill="#e2e8f0" stroke="#64748b" stroke-width="0.8" data-glyph="unknown"/><text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="{:.1}" font-weight="bold" fill="#334155">?</text>"##,
            cy - 6.0 * s,
            12.0 * s,
            12.0 * s,
            x + 6.0 * s,
            cy + 3.5 * s,
            10.0 * s
        ));
    } else if glyph == Glyph::Folder {
        svg.push_str(&format!(
            r##"<path d="M {x:.1} {:.1} h {:.1} l {:.1} {:.1} h {:.1} v {:.1} h {:.1} z" fill="#fbbf24" stroke="#b45309" stroke-width="0.8" data-glyph="folder"/>"##,
            cy - 5.0 * s,
            5.0 * s,
            1.5 * s,
            2.0 * s,
            7.5 * s,
            9.0 * s,
            -14.0 * s
        ));
    } else {
        svg.push_str(&format!(
            r##"<path d="M {:.1} {:.1} h {:.1} l {:.1} {:.1} v {:.1} h {:.1} z" fill="#f8fafc" stroke="#64748b" stroke-width="0.8" data-glyph="file"/>"##,
            x + 1.5 * s,
            cy - 6.5 * s,
            7.5 * s,
            3.5 * s,
            3.5 * s,
            9.5 * s,
            -11.0 * s
        ));
    }
}

pub(super) fn render_treeview_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let parsed = parse_tree(source);
    if parsed.is_empty() {
        return None;
    }
    // mermaid.js hangs every entry off an implicit `/` root and drops the trailing `/` of
    // directory names; `dirs` remembers which entries are directories.
    let mut entries = vec![TreeEntry { name: "/".into(), depth: 0, classes: Vec::new(), icon: None, comment: None }];
    let mut dirs = vec![true];
    for (i, e) in parsed.iter().enumerate() {
        let dir = e.name.ends_with('/') || parsed.get(i + 1).is_some_and(|n| n.depth > e.depth);
        let name = if e.name.len() > 1 { e.name.trim_end_matches('/').to_string() } else { e.name.clone() };
        entries.push(TreeEntry { name, depth: e.depth + 1, ..e.clone() });
        dirs.push(dir);
    }
    let number = |key: &str, default: f32, lo: f32, hi: f32| config.option(key).and_then(px_number).map_or(default, |v| (v as f32).clamp(lo, hi));
    let indent = number("treeView.rowIndent", 22.0, 6.0, 200.0);
    let thickness = number("treeView.lineThickness", 1.0, 0.5, 8.0);
    let font = config.vars.get("treeView.labelFontSize").and_then(|v| px_number(v)).map_or(14.0, |v| (v as f32).clamp(8.0, 40.0));
    let label_color = config.color_var("treeView.labelColor").unwrap_or_else(|| "#0f172a".to_string());
    let line_color = config.color_var("treeView.lineColor").unwrap_or_else(|| "#94a3b8".to_string());
    let show_icons = config.flag("treeView.showIcons", false);
    let row = (font * 1.75).max(20.0);
    let margin = 16.0;
    // `filenameIcons.<name>` / `extensionIcons.<.ext>` from the config.
    let mapped = |name: &str| -> Option<String> {
        if let Some(v) = config.option(&format!("treeView.filenameIcons.{name}")) {
            return Some(unquote(v));
        }
        let ext = name.rfind('.').map(|d| &name[d..])?;
        config.option(&format!("treeView.extensionIcons.{ext}")).map(unquote)
    };
    let glyph_of = |i: usize| -> Option<Glyph> {
        let e = &entries[i];
        let plain = if dirs[i] { Glyph::Folder } else { Glyph::File };
        match e.icon.clone().or_else(|| if show_icons && i > 0 { mapped(&e.name) } else { None }).as_deref() {
            Some("none") => None,
            Some(_) => Some(Glyph::Unknown),
            None if show_icons => Some(plain),
            None => None,
        }
    };
    let glyph_w = font + 6.0;
    let label_x = |i: usize| margin + entries[i].depth as f32 * indent + if glyph_of(i).is_some() { glyph_w } else { 0.0 };
    let label_end = (0..entries.len()).map(|i| label_x(i) + estimate_text_width(&entries[i].name, font) + 8.0).fold(0.0f32, f32::max);
    let comment_x = label_end + 24.0;
    let comment_w = entries.iter().filter_map(|e| e.comment.as_deref()).map(|c| estimate_text_width(&format!("# {c}"), font * 0.9)).fold(0.0f32, f32::max);
    let width = (if comment_w > 0.0 { comment_x + comment_w } else { label_end } + margin).ceil();
    let height = (margin * 2.0 + entries.len() as f32 * row).ceil();
    let cy = |i: usize| margin + (i as f32 + 0.5) * row;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="sans-serif"><rect width="{width}" height="{height}" fill="#ffffff"/>"##
    ));
    // Connectors: from each entry down to its last child, with a stub into every child.
    for (i, e) in entries.iter().enumerate() {
        let children: Vec<usize> = (i + 1..entries.len()).take_while(|&j| entries[j].depth > e.depth).filter(|&j| entries[j].depth == e.depth + 1).collect();
        let Some(&last) = children.last() else { continue };
        let x = margin + e.depth as f32 * indent + indent * 0.4;
        let mut d = format!("M {x:.1} {:.1} V {:.1}", cy(i) + row * 0.35, cy(last));
        for &c in &children {
            d.push_str(&format!(" M {x:.1} {:.1} H {:.1}", cy(c), margin + entries[c].depth as f32 * indent - 3.0));
        }
        svg.push_str(&format!(r##"<path d="{d}" fill="none" stroke="{line_color}" stroke-width="{thickness}" data-role="connector"/>"##));
    }
    for (i, e) in entries.iter().enumerate() {
        let x0 = margin + e.depth as f32 * indent;
        let y = cy(i);
        let lx = label_x(i);
        if !e.classes.is_empty() {
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="4" fill="#fef3c7" stroke="#f59e0b" stroke-width="1" data-class="{}"/>"##,
                lx - 4.0,
                y - row * 0.42,
                estimate_text_width(&e.name, font) + 8.0,
                row * 0.84,
                escape_attr(&e.classes.join(" "))
            ));
        }
        if let Some(glyph) = glyph_of(i) {
            push_glyph(&mut svg, glyph, x0, y, font);
        }
        let bold = if dirs[i] { "600" } else { "normal" };
        svg.push_str(&format!(
            r##"<text x="{lx:.1}" y="{:.1}" font-size="{font}" font-weight="{bold}" fill="{label_color}" data-item="{}">{}</text>"##,
            y + font * 0.35,
            escape_attr(&e.name),
            escape_xml(&e.name)
        ));
        if let Some(c) = &e.comment {
            svg.push_str(&format!(
                r##"<text x="{comment_x:.1}" y="{:.1}" font-size="{:.1}" font-style="italic" fill="#64748b">{}</text>"##,
                y + font * 0.35,
                font * 0.9,
                escape_xml(&format!("# {c}"))
            ));
        }
    }
    svg.push_str("</svg>");
    Some((svg.into_bytes(), width as u32, height as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn tree_depths_from_box_drawing_and_indentation() {
        let boxed = parse_tree("treeView-beta\n├── src/\n│   ├── index.ts\n│   └── utils.ts\n└── README.md");
        assert_eq!(boxed.iter().map(|e| (e.name.as_str(), e.depth)).collect::<Vec<_>>(), vec![("src/", 0), ("index.ts", 1), ("utils.ts", 1), ("README.md", 0)]);
        let indented = parse_tree("treeView-beta\n    \"packages\"\n        \"mermaid\"\n            \"src\"\n        \"parser\"");
        assert_eq!(indented.iter().map(|e| (e.name.as_str(), e.depth)).collect::<Vec<_>>(), vec![("packages", 0), ("mermaid", 1), ("src", 2), ("parser", 1)]);
    }

    #[test]
    fn tree_items_classes_icons_and_comments() {
        assert_eq!(
            parse_tree_item("App.tsx :::highlight icon(logos:react) ## main component"),
            ("App.tsx".into(), vec!["highlight".into()], Some("logos:react".into()), Some("main component".into()))
        );
        assert_eq!(parse_tree_item(".env ## environment variables").0, ".env");
    }

    #[test]
    fn tree_svg_rows_connectors_and_glyphs() {
        let svg = svg_of("---\nconfig:\n  treeView:\n    showIcons: true\n    rowIndent: 40\n---\ntreeView-beta\n    src/\n        index.js icon(none)\n        App.tsx :::highlight\n    package.json");
        assert!(text_x(&svg, "App.tsx") - text_x(&svg, "src") >= 40.0);
        assert!(text_y(&svg, "App.tsx") > text_y(&svg, "src") && text_y(&svg, "src") > text_y(&svg, "/"));
        assert!(text_x(&svg, "src") > text_x(&svg, "/"));
        assert_eq!(svg.matches(r#"data-glyph="folder""#).count(), 2);
        assert_eq!(svg.matches(r#"data-glyph="file""#).count(), 2);
        assert!(svg.contains(r#"data-class="highlight""#) && svg.contains(r#"data-role="connector""#));
    }

    #[test]
    fn tree_unresolved_icons_draw_a_question_mark() {
        let svg = svg_of("---\nconfig:\n  treeView:\n    showIcons: true\n    filenameIcons:\n      Dockerfile: docker\n    extensionIcons:\n      .ts: typescript\n      .txt: none\n---\ntreeView-beta\n    a.ts\n    Dockerfile\n    notes.txt\n    b.md icon(logos:react)\n    c.md");
        assert_eq!(svg.matches(r#"data-glyph="unknown""#).count(), 3);
        assert_eq!(svg.matches(r#"data-glyph="file""#).count(), 1);
    }
}
