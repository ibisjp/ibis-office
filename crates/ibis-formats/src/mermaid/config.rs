// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Frontmatter (`title`, `config`), YAML subset and themes.

use super::*;

pub(super) fn strip_mermaid_frontmatter(source: &str) -> &str {
    let mut s = source.trim();
    if let Some(after_open) = s.strip_prefix("---") {
        if after_open.starts_with('\n') || after_open.starts_with("\r\n") {
            let mut offset = 3;
            for line in after_open.split_inclusive('\n') {
                offset += line.len();
                if line.trim() == "---" {
                    s = s[offset..].trim_start();
                    break;
                }
            }
        }
    }
    // Advance past any leading blank or `%%` directive/comment lines so line 0 is the diagram header.
    loop {
        let trimmed_start = s.trim_start();
        if trimmed_start.starts_with("%%") {
            if let Some(nl) = trimmed_start.find('\n') {
                s = &trimmed_start[nl + 1..];
                continue;
            }
            return "";
        }
        return trimmed_start;
    }
}

/// The top-level `title:` of a YAML frontmatter block (`---` … `---`), unquoted.
/// Nested keys (such as `config:` children) are ignored.
pub(super) fn mermaid_frontmatter_title(source: &str) -> Option<String> {
    let after_open = source.trim_start().strip_prefix("---")?;
    if !(after_open.starts_with('\n') || after_open.starts_with("\r\n")) {
        return None;
    }
    for line in after_open.lines().skip(1) {
        if line.trim() == "---" {
            break;
        }
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title:") {
            let title = unquote(rest.trim());
            return (!title.is_empty()).then_some(title);
        }
    }
    None
}

/// Colour themes from mermaid's `config.theme`. `base` and `default` keep the built-in palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MermaidTheme {
    Default,
    Base,
    Dark,
    Forest,
    Neutral,
}

/// The parts of a frontmatter `config:` block this renderer reads. Everything is kept as text;
/// each renderer picks the keys it knows and ignores the rest, so unknown options are harmless.
#[derive(Debug, Clone, Default)]
pub(super) struct MermaidConfig {
    pub(super) theme: Option<MermaidTheme>,
    /// `themeVariables.*` without the prefix (`git0`, `commitLabelColor`, `xyChart.titleColor`).
    pub(super) vars: HashMap<String, String>,
    /// Every other leaf as a dotted path below `config` (`gitGraph.showBranches`, `logLevel`).
    pub(super) options: HashMap<String, String>,
}

impl MermaidConfig {
    pub(super) fn option(&self, key: &str) -> Option<&str> {
        self.options.get(key).map(String::as_str)
    }

    /// A boolean option; anything other than `true`/`false` falls back to `default`.
    pub(super) fn flag(&self, key: &str, default: bool) -> bool {
        match self.option(key).map(str::to_ascii_lowercase).as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        }
    }

    /// A theme variable that is safe to put in an SVG attribute (see [`safe_css_value`]).
    pub(super) fn color_var(&self, key: &str) -> Option<String> {
        self.vars.get(key).and_then(|v| safe_css_value(v))
    }
}

/// A CSS value made only of characters that cannot break out of an SVG attribute
/// (`#ff0000`, `rgb(1, 2, 3)`, `lightblue`, `12px`).
pub(super) fn safe_css_value(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | '.' | '%' | '(' | ')' | ',' | ' ' | '-')))
    .then(|| value.to_string())
}

/// A hex colour (`#rgb`, `#rrggbb`, or the same without `#`, as in `quadrant1TextFill: "ff0000"`)
/// as lowercase `#rrggbb`.
pub(super) fn hex_color(value: &str) -> Option<String> {
    let v = value.trim().trim_matches(|c| c == '"' || c == '\'');
    let h = v.strip_prefix('#').unwrap_or(v);
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match h.len() {
        6 => Some(format!("#{}", h.to_ascii_lowercase())),
        3 => Some(h.chars().flat_map(|c| [c, c]).fold(String::from("#"), |mut s, c| {
            s.push(c.to_ascii_lowercase());
            s
        })),
        _ => None,
    }
}

/// A colour theme variable: a hex colour (also without `#`), or any other safe CSS colour name.
pub(super) fn css_color(value: &str) -> Option<String> {
    hex_color(value).or_else(|| safe_css_value(value).filter(|v| v.chars().all(|c| c.is_ascii_alphabetic())))
}

/// `5px`, `5` or `"5px"` as a number of px.
pub(super) fn px_number(value: &str) -> Option<f64> {
    let v = value.trim().trim_matches(|c| c == '"' || c == '\'');
    let v = v.strip_suffix("px").unwrap_or(v).trim();
    v.parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0)
}

/// The lines between the opening and closing `---` of a YAML frontmatter block.
pub(super) fn frontmatter_lines(source: &str) -> Option<Vec<&str>> {
    let after_open = source.trim_start().strip_prefix("---")?;
    if !(after_open.starts_with('\n') || after_open.starts_with("\r\n")) {
        return None;
    }
    let mut out = Vec::new();
    for line in after_open.lines().skip(1) {
        if line.trim() == "---" {
            return Some(out);
        }
        out.push(line);
    }
    None
}

/// Index of the first `ch` outside single or double quotes.
pub(super) fn find_unquoted(s: &str, ch: char) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (idx, c) in s.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == ch => return Some(idx),
            None => {}
        }
    }
    None
}

/// Leaf `key: value` pairs of a small YAML subset (block mappings, scalars, quoted strings that
/// may span lines, `#` comments, block scalars skipped), as dotted paths.
pub(super) fn yaml_leaves(lines: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        i += 1;
        let content = line.trim_start();
        if content.is_empty() || content.starts_with('#') || content.starts_with('-') {
            continue;
        }
        let indent = line.len() - content.len();
        let Some(colon) = find_unquoted(content, ':') else {
            continue;
        };
        let key = unquote(&content[..colon]);
        let mut value = content[colon + 1..].trim().to_string();
        if let Some(hash) = find_unquoted(&value, '#')
            && (hash == 0 || value[..hash].ends_with(char::is_whitespace))
        {
            value.truncate(hash);
            value = value.trim().to_string();
        }
        // A quoted string left open continues on the following lines.
        for q in ['"', '\''] {
            if value.starts_with(q) && (value.len() == 1 || !value.ends_with(q)) {
                while i < lines.len() && !value.ends_with(q) {
                    value.push(' ');
                    value.push_str(lines[i].trim());
                    i += 1;
                }
            }
        }
        while stack.last().is_some_and(|(level, _)| *level >= indent) {
            stack.pop();
        }
        if value.is_empty() {
            stack.push((indent, key));
            continue;
        }
        if matches!(value.as_str(), "|" | "|-" | "|+" | ">" | ">-" | ">+") {
            while i < lines.len() {
                let next = lines[i];
                let next_content = next.trim_start();
                if !next_content.is_empty() && next.len() - next_content.len() <= indent {
                    break;
                }
                i += 1;
            }
            value.clear();
        }
        let mut path: Vec<&str> = stack.iter().map(|(_, k)| k.as_str()).collect();
        path.push(&key);
        out.push((path.join("."), unquote(&value)));
    }
    out
}

/// The frontmatter `config:` block, if any.
pub(super) fn mermaid_frontmatter_config(source: &str) -> MermaidConfig {
    let mut config = MermaidConfig::default();
    let Some(lines) = frontmatter_lines(source) else {
        return config;
    };
    for (path, value) in yaml_leaves(&lines) {
        let Some(key) = path.strip_prefix("config.") else {
            continue;
        };
        if key == "theme" {
            config.theme = match value.trim().to_ascii_lowercase().as_str() {
                "dark" => Some(MermaidTheme::Dark),
                "forest" => Some(MermaidTheme::Forest),
                "neutral" => Some(MermaidTheme::Neutral),
                "base" => Some(MermaidTheme::Base),
                "default" => Some(MermaidTheme::Default),
                _ => None,
            };
        } else if let Some(var) = key.strip_prefix("themeVariables.") {
            config.vars.insert(var.to_string(), value);
        } else {
            config.options.insert(key.to_string(), value);
        }
    }
    config
}

// ----------------------------------------------------------------------------
// Themes. The renderers draw with one built-in palette; a theme maps those colours
// (and only those, so `style`/`classDef` colours stay as written) to the theme's.
// ----------------------------------------------------------------------------

pub(super) fn hex_rgb(hex: &str) -> Option<(f32, f32, f32)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|b| b as f32 / 255.0);
    Some((v(0)?, v(2)?, v(4)?))
}

pub(super) fn rgb_hsl((r, g, b): (f32, f32, f32)) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

pub(super) fn hsl_hex(h: f32, s: f32, l: f32) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", to(r), to(g), to(b))
}

/// Built-in colours a theme may remap: the shared slate/blue palette and the light fills.
pub(super) const THEMED_COLORS: &[&str] = &[
    "#ffffff", "#f8fafc", "#f1f5f9", "#e2e8f0", "#cbd5e1", "#94a3b8", "#64748b", "#475569", "#334155",
    "#1e293b", "#0f172a", "#eff6ff", "#bfdbfe", "#93c5fd", "#3b82f6", "#2563eb", "#f5f3ff", "#7c3aed",
    "#fefce8", "#ca8a04", "#f0fdf4", "#16a34a", "#ecfeff", "#0891b2", "#f0fdfa", "#0f766e", "#fef9c3",
    "#854d0e", "#dcfce7", "#fee2e2", "#fff7ed", "#fdf2f8", "#cffafe",
];

pub(super) fn theme_color(theme: MermaidTheme, hex: &str) -> Option<String> {
    let (h, s, l) = rgb_hsl(hex_rgb(hex)?);
    let blue = (195.0..=265.0).contains(&h) && s > 0.25;
    match theme {
        MermaidTheme::Default | MermaidTheme::Base => None,
        MermaidTheme::Dark => {
            let l2 = if l > 0.75 {
                0.1 + (1.0 - l) * 2.0
            } else if l < 0.3 {
                0.95 - l
            } else if s < 0.3 {
                1.0 - l * 0.9
            } else {
                (l + 0.08).min(0.75)
            };
            Some(hsl_hex(h, s.min(0.6), l2))
        }
        // mermaid's forest theme: green nodes and borders.
        MermaidTheme::Forest => blue.then(|| hsl_hex(95.0, s.min(0.65), l)),
        // neutral: greys only.
        MermaidTheme::Neutral => (s > 0.0).then(|| hsl_hex(h, 0.0, l)),
    }
}

/// Swap every `="#rrggbb"` attribute value found in `table`.
pub(super) fn remap_svg_colors(svg: &str, table: &HashMap<String, String>) -> String {
    if table.is_empty() {
        return svg.to_string();
    }
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find("=\"#") {
        let value_start = at + 2;
        out.push_str(&rest[..value_start]);
        rest = &rest[value_start..];
        let end = rest[1..].find('"').map_or(rest.len(), |e| e + 1);
        let value = &rest[..end];
        match table.get(&value.to_ascii_lowercase()) {
            Some(new) => out.push_str(new),
            None => out.push_str(value),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Apply `config.theme` and the generic `themeVariables` (`primaryColor`, `primaryBorderColor`,
/// `primaryTextColor`, `lineColor`, `background`) to one of this module's SVGs.
pub(super) fn apply_mermaid_theme(bytes: Vec<u8>, config: &MermaidConfig) -> Vec<u8> {
    let mut table: HashMap<String, String> = HashMap::new();
    if let Some(theme) = config.theme {
        for &hex in THEMED_COLORS {
            if let Some(new) = theme_color(theme, hex) {
                table.insert(hex.to_string(), new);
            }
        }
    }
    for (var, hex) in [
        ("background", "#ffffff"),
        ("primaryColor", "#eff6ff"),
        ("primaryBorderColor", "#2563eb"),
        ("primaryTextColor", "#0f172a"),
        ("lineColor", "#475569"),
    ] {
        if let Some(value) = config.color_var(var) {
            table.insert(hex.to_string(), value);
        }
    }
    if table.is_empty() {
        return bytes;
    }
    match String::from_utf8(bytes) {
        Ok(svg) => remap_svg_colors(&svg, &table).into_bytes(),
        Err(e) => e.into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn frontmatter_config_reads_a_small_yaml_subset() {
        let src = "---\ntitle: T\nconfig:\n  logLevel: 'debug'\n  theme: 'Forest'  # a comment\n  themeVariables:\n    'git0': '#ff0000'\n    commitLabelFontSize: 14px\n  themeCSS: \" .a { fill: red }\n    .b {}\n  \"\n  gantt:\n    topAxis: true  #false\n  notes: |\n    free text: here\n  gitGraph:\n    showBranches: false\n---\ngitGraph\n  commit\n";
        let config = mermaid_frontmatter_config(src);
        assert_eq!(config.theme, Some(MermaidTheme::Forest));
        assert_eq!(config.vars.get("git0").map(String::as_str), Some("#ff0000"));
        assert_eq!(config.vars.get("commitLabelFontSize").map(String::as_str), Some("14px"));
        assert_eq!(config.option("logLevel"), Some("debug"));
        assert_eq!(config.option("gantt.topAxis"), Some("true"));
        assert!(!config.flag("gitGraph.showBranches", true));
        assert!(config.option("themeCSS").is_some_and(|css| css.contains(".b")));
        // Block scalar content is skipped, not read as keys.
        assert_eq!(config.option("notes"), Some(""));
        assert!(!config.options.keys().any(|k| k.contains("free text")));
        // Indented frontmatter keys and no frontmatter at all.
        let indented = mermaid_frontmatter_config("---\n  config:\n    class:\n      hideEmptyMembersBox: true\n---\nclassDiagram\n  class A\n");
        assert!(indented.flag("class.hideEmptyMembersBox", false));
        assert!(mermaid_frontmatter_config("flowchart LR\n  A --> B\n").options.is_empty());
        // Unknown options are ignored and the diagram still renders.
        assert!(render_mermaid("---\nconfig:\n  flowchart:\n    curve: stepAfter\n    nonsense: [1, 2]\n---\nflowchart LR\n  A --> B\n").is_some());
        // Theme variables only reach the SVG when they are plain colour values.
        let hostile = "---\nconfig:\n  themeVariables:\n    primaryColor: 'red\" onload=\"alert(1)'\n---\nflowchart LR\n  A --> B\n";
        let svg = svg_of(hostile);
        assert!(!svg.contains("onload") && !svg.contains("alert"), "{svg}");
        assert_eq!(safe_css_value("rgb(1, 2, 3)").as_deref(), Some("rgb(1, 2, 3)"));
        assert_eq!(safe_css_value("url(javascript:x)"), None);
    }

    #[test]
    fn themes_remap_the_builtin_palette_but_keep_user_styles() {
        let body = "flowchart LR\n  A --> B\n  style B fill:#ff99ff\n";
        let plain = svg_of(body);
        assert!(plain.contains(r##"fill="#eff6ff""##) && plain.contains(r##"stroke="#2563eb""##));
        let themed = |theme: &str| svg_of(&format!("---\nconfig:\n  theme: {theme}\n---\n{body}"));
        assert_eq!(themed("default"), plain);
        assert_eq!(themed("base"), plain);

        let dark = themed("dark");
        let bg = tag_with(&dark, "<rect width=")[0];
        let fill = bg.split("fill=\"").nth(1).unwrap().split('"').next().unwrap();
        let (_, _, l) = rgb_hsl(hex_rgb(fill).unwrap());
        assert!(l < 0.2, "dark background {fill}");
        let label = tag_with(&dark, ">A<")[0];
        let ink = label.split("fill=\"").nth(1).unwrap().split('"').next().unwrap();
        assert!(rgb_hsl(hex_rgb(ink).unwrap()).2 > 0.7, "light text {ink}");
        // A `style` colour is the author's, not the palette's.
        assert!(dark.contains(r##"fill="#ff99ff""##));

        let forest = themed("forest");
        assert!(!forest.contains(r##"stroke="#2563eb""##));
        let stroke = tag_with(&forest, "<rect x=")[0].split("stroke=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
        let (h, _, _) = rgb_hsl(hex_rgb(&stroke).unwrap());
        assert!((70.0..140.0).contains(&h), "green border {stroke}");

        let neutral = themed("neutral");
        let stroke = tag_with(&neutral, "<rect x=")[0].split("stroke=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
        assert!(rgb_hsl(hex_rgb(&stroke).unwrap()).1 < 0.01, "grey border {stroke}");

        let vars = svg_of(&format!("---\nconfig:\n  themeVariables:\n    primaryColor: '#fde68a'\n    lineColor: darkred\n---\n{body}"));
        assert!(vars.contains(r##"fill="#fde68a""##) && vars.contains(r#"stroke="darkred""#), "{vars}");
        // Unknown theme names change nothing.
        assert_eq!(themed("redux-color"), plain);
        assert_eq!(hsl_hex(0.0, 0.0, 1.0), "#ffffff");
        assert_eq!(hsl_hex(120.0, 1.0, 0.5), "#00ff00");
    }
}
