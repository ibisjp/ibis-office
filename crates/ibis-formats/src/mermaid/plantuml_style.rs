// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Lightweight, CSS-engine-free PlantUML styling & inline Creole helpers built on top of the
//! existing Mermaid theme remapper (`config.rs`) and inline markdown text renderer (`text.rs`).

use super::config::{MermaidConfig, MermaidTheme, apply_mermaid_theme};
use super::plantuml::{PlantUmlNativeRender, strip_block_comments, strip_prefix_ci};
use super::MermaidOutput;

/// Extract widely-used global PlantUML theme/skinparam directives into a [`MermaidConfig`]
/// plus the 1-based line numbers that were handled (so callers don't report them as unsupported).
pub(super) fn extract_plantuml_theme_config(source: &str) -> (MermaidConfig, Vec<usize>) {
    let mut cfg = MermaidConfig::default();
    let mut handled_lines = Vec::new();
    let mut in_comment = false;

    for (idx, raw) in source.lines().enumerate() {
        let line_no = idx + 1;
        let cleaned = strip_block_comments(raw, &mut in_comment);
        let trimmed = cleaned.trim();
        if trimmed.is_empty() || trimmed.starts_with('\'') {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("@end") {
            break;
        }

        // `!theme <name>` for well-known built-in PlantUML themes.
        if let Some(rest) = strip_prefix_ci(trimmed, "!theme ") {
            let name = rest.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
            if let Some(mapped) = map_plantuml_theme(&name) {
                if let Some(t) = mapped {
                    cfg.theme = Some(t);
                }
                handled_lines.push(line_no);
            }
            continue;
        }

        // Single-line `skinparam <key> <value>` for widely-used global styling directives.
        if let Some(rest) = strip_prefix_ci(trimmed, "skinparam ") {
            if trimmed.ends_with('{') {
                continue;
            }
            let mut parts = rest.split_whitespace();
            let Some(key) = parts.next() else { continue };
            let Some(val) = parts.next() else { continue };
            let key_lower = key.to_ascii_lowercase();
            if key_lower == "monochrome" {
                if val.eq_ignore_ascii_case("true") {
                    cfg.theme = Some(MermaidTheme::Neutral);
                    handled_lines.push(line_no);
                } else if val.eq_ignore_ascii_case("false") {
                    handled_lines.push(line_no);
                }
                continue;
            }
            if let Some(var_name) = map_skinparam_color_var(&key_lower)
                && let Some(hex) = normalize_plantuml_color(val)
            {
                cfg.vars.insert(var_name.to_string(), hex);
                handled_lines.push(line_no);
            }
        }
    }

    (cfg, handled_lines)
}

/// Apply lightweight global theme/skinparam color remapping to any rendered PlantUML SVG
/// and suppress diagnostics for lines that were actually applied.
pub(super) fn apply_plantuml_lightweight_theme(
    source: &str,
    mut render: PlantUmlNativeRender,
) -> PlantUmlNativeRender {
    let (cfg, handled_lines) = extract_plantuml_theme_config(source);
    if !handled_lines.is_empty() {
        render
            .unsupported
            .retain(|u| !handled_lines.contains(&u.line));
    }
    if (cfg.theme.is_some() || !cfg.vars.is_empty())
        && let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render.output.take()
    {
        let themed = apply_mermaid_theme(bytes, &cfg);
        render.output = Some(MermaidOutput::Svg {
            bytes: themed,
            width_px,
            height_px,
        });
    }
    render
}

/// Map well-known PlantUML `!theme` names to the closest built-in Mermaid theme.
/// Returns `Some(Some(theme))` for mapped themes, `Some(None)` for default-equivalent themes,
/// or `None` for unrecognized/external themes.
fn map_plantuml_theme(name: &str) -> Option<Option<MermaidTheme>> {
    match name {
        "dark" | "cyborg" | "cyborg-outline" | "hacker" | "superhero" | "superhero-outline"
        | "slate" | "black-knight" | "carbon-gray" | "crt-amber" | "crt-green" | "vibrant" => {
            Some(Some(MermaidTheme::Dark))
        }
        "plain" | "monochrome" | "sketchy" | "sketchy-outline" | "silver" | "lightgray"
        | "toy" => Some(Some(MermaidTheme::Neutral)),
        "minty" | "sandstone" | "united" | "spacelab" => Some(Some(MermaidTheme::Forest)),
        "cerulean" | "cerulean-outline" | "bluegray" | "mars" | "materia" | "materia-outline"
        | "amiga" | "aws-orange" => Some(None),
        _ => None,
    }
}

fn map_skinparam_color_var(key_lower: &str) -> Option<&'static str> {
    match key_lower {
        "backgroundcolor" => Some("background"),
        "arrowcolor" => Some("lineColor"),
        "defaultfontcolor" => Some("primaryTextColor"),
        "classbackgroundcolor"
        | "activitybackgroundcolor"
        | "statebackgroundcolor"
        | "componentbackgroundcolor"
        | "participantbackgroundcolor"
        | "objectbackgroundcolor"
        | "nodebackgroundcolor"
        | "usecasebackgroundcolor" => Some("primaryColor"),
        "classbordercolor"
        | "activitybordercolor"
        | "statebordercolor"
        | "componentbordercolor"
        | "participantbordercolor"
        | "objectbordercolor"
        | "nodebordercolor"
        | "usecasebordercolor" => Some("primaryBorderColor"),
        _ => None,
    }
}

/// Normalize a PlantUML color token (`#EEE`, `#dbeafe`, `#LightBlue`, `LightGreen`, `#back:Pink`)
/// into a valid `#rrggbb` hex color for Mermaid `style`/`classDef`/`themeVariables`.
pub(super) fn normalize_plantuml_color(raw: &str) -> Option<String> {
    let mut s = raw.trim().trim_end_matches(';');
    if let Some(after) = s.strip_prefix('#') {
        s = after;
    }
    // Strip PlantUML `back:` or `line:` modifiers when present (e.g. `#back:Pink`).
    if let Some(after) = strip_prefix_ci(s, "back:") {
        s = after;
    }
    // Ignore gradient syntax (`#A-B` / `#A|B`) by taking the first stop.
    if let Some((first, _)) = s.split_once(['-', '|', '/', '\\']) {
        s = first;
    }
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(format!("#{}", s.to_ascii_lowercase()));
    }
    if s.len() == 3 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let chars: Vec<char> = s.to_ascii_lowercase().chars().collect();
        return Some(format!(
            "#{0}{0}{1}{1}{2}{2}",
            chars[0], chars[1], chars[2]
        ));
    }
    let named = match s.to_ascii_lowercase().as_str() {
        "white" => "#ffffff",
        "black" => "#000000",
        "red" => "#ef4444",
        "green" => "#22c55e",
        "blue" => "#3b82f6",
        "yellow" => "#fde047",
        "orange" => "#fb923c",
        "purple" => "#a855f7",
        "pink" => "#fbcfe8",
        "cyan" | "aqua" => "#a5f3fc",
        "gold" => "#fde68a",
        "gray" | "grey" => "#94a3b8",
        "lightgray" | "lightgrey" | "silver" => "#e2e8f0",
        "darkgray" | "darkgrey" => "#475569",
        "lightblue" | "aliceblue" | "powderblue" => "#dbeafe",
        "skyblue" | "lightskyblue" => "#bae6fd",
        "lightsteelblue" => "#cbd5e1",
        "lightgreen" | "palegreen" => "#dcfce7",
        "lightyellow" | "cornsilk" | "lemonchiffon" | "ivory" => "#fef9c3",
        "lightpink" | "mistyrose" | "lightcoral" | "salmon" => "#ffe4e6",
        "lightcyan" => "#cffafe",
        "lavender" | "thistle" | "plum" | "violet" => "#f3e8ff",
        "antiquewhite" | "wheat" | "beige" | "moccasin" | "papayawhip" | "seashell" | "linen" => {
            "#fef3c7"
        }
        "khaki" => "#fef08a",
        "tomato" => "#fca5a5",
        "transparent" => "none",
        _ => return None,
    };
    Some(named.to_string())
}

/// Extract a trailing PlantUML `#Color` token from a declaration head (returning `(head, Some(hex))`
/// if recognized, or `(head, None)` when no color was present).
pub(super) fn split_and_normalize_color(s: &str) -> (&str, Option<String>, bool) {
    let trimmed = s.trim_end();
    if let Some(pos) = trimmed.rfind(" #") {
        let token = &trimmed[pos + 1..];
        if !token.contains(char::is_whitespace) && token.len() > 1 {
            return (
                trimmed[..pos].trim_end(),
                normalize_plantuml_color(token),
                true,
            );
        }
    }
    (trimmed, None, false)
}

/// Normalize basic PlantUML Creole inline formatting (`//italic//`) into Mermaid markdown
/// (`*italic*`) while leaving URLs (`https://...`) and `**bold**` intact.
pub(super) fn normalize_creole_inline(s: &str) -> String {
    if !s.contains("//") {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find("//") {
        // Skip URL protocol separators like `http://` or `https://`.
        if open > 0 && rest.as_bytes()[open - 1] == b':' {
            out.push_str(&rest[..open + 2]);
            rest = &rest[open + 2..];
            continue;
        }
        let after_open = &rest[open + 2..];
        if let Some(close) = after_open.find("//")
            && close > 0
            && !after_open[..close].contains("://")
        {
            out.push_str(&rest[..open]);
            out.push('*');
            out.push_str(&after_open[..close]);
            out.push('*');
            rest = &after_open[close + 2..];
            continue;
        }
        out.push_str(&rest[..open + 2]);
        rest = &rest[open + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use crate::mermaid::{MermaidOutput, render_plantuml};

    #[test]
    fn applies_lightweight_theme_skinparam_element_colors_and_creole() {
        let themed_seq = "\
@startuml
!theme plain
skinparam backgroundColor #f8fafc
skinparam ArrowColor #0f766e
Alice -> Bob : **重要** //確認// https://example.com
@enduml
";
        let res = render_plantuml(themed_seq);
        assert!(res.is_lossless(), "unexpected diagnostics: {:?}", res.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res.output else {
            panic!("expected SVG for themed sequence");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        assert!(svg.contains("#f8fafc"), "expected backgroundColor applied: {svg}");
        assert!(svg.contains("#0f766e"), "expected ArrowColor applied: {svg}");

        let mono_comp = "\
@startuml
skinparam monochrome true
component \"Web\" as W
database \"DB\" as DB
W --> DB : SQL
@enduml
";
        let res_m = render_plantuml(mono_comp);
        assert!(res_m.is_lossless(), "unexpected diagnostics: {:?}", res_m.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_m.output else {
            panic!("expected SVG for monochrome component diagram");
        };
        let svg_m = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_m).is_ok(), "invalid SVG");
        assert!(svg_m.contains("#585858"), "expected monochrome stroke remap: {svg_m}");

        let colored_comp = "\
@startuml
component \"API ゲートウェイ\" as API #LightBlue
database \"主 DB\" as DB #LightGreen
API --> DB : //SQL//
@enduml
";
        let res_c = render_plantuml(colored_comp);
        assert!(res_c.is_lossless(), "unexpected diagnostics: {:?}", res_c.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_c.output else {
            panic!("expected SVG for colored component diagram");
        };
        let svg_c = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_c).is_ok(), "invalid SVG");
        assert!(svg_c.contains("#dbeafe"), "expected LightBlue fill on API: {svg_c}");
        assert!(svg_c.contains("#dcfce7"), "expected LightGreen fill on DB: {svg_c}");

        let colored_act = "\
@startuml
start
#Pink:審査待ち;
#LightGreen:承認完了;
stop
@enduml
";
        let res_a = render_plantuml(colored_act);
        assert!(res_a.is_lossless(), "unexpected diagnostics: {:?}", res_a.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_a.output else {
            panic!("expected SVG for colored activity diagram");
        };
        let svg_a = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_a).is_ok(), "invalid SVG");
        assert!(svg_a.contains("#fbcfe8") && svg_a.contains("#dcfce7"), "{svg_a}");

        let colored_state = "\
@startuml
state \"稼働中\" as Running #LightYellow
[*] --> Running
@enduml
";
        let res_s = render_plantuml(colored_state);
        assert!(res_s.is_lossless(), "unexpected diagnostics: {:?}", res_s.unsupported);
        let Some(MermaidOutput::Svg { bytes, .. }) = res_s.output else {
            panic!("expected SVG for colored state diagram");
        };
        let svg_s = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg_s).is_ok(), "invalid SVG");
        assert!(svg_s.contains("#fef9c3"), "expected LightYellow fill on state: {svg_s}");
    }

    #[test]
    fn creole_normalizer_converts_italic_without_breaking_urls() {
        assert_eq!(
            super::normalize_creole_inline("**bold** //italic// https://sorane.dev/docs"),
            "**bold** *italic* https://sorane.dev/docs"
        );
    }
}
