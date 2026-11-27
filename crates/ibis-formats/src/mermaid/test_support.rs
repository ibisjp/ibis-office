// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Assertions on rendered SVG shared by the renderer tests.

use super::*;

pub(super) fn svg_of(source: &str) -> String {
    let Some(MermaidOutput::Svg { bytes, .. }) = render_mermaid(source) else {
        panic!("expected svg:\n{source}");
    };
    String::from_utf8(bytes).expect("svg utf-8")
}

pub(super) fn text_x(svg: &str, label: &str) -> f32 {
    let needle = format!(">{label}<");
    let at = svg.find(&needle).unwrap_or_else(|| panic!("missing {label}"));
    let head = &svg[..at];
    let x_at = head.rfind("x=\"").unwrap_or_else(|| panic!("no x for {label}"));
    let rest = &head[x_at + 3..];
    let end = rest.find('"').unwrap();
    rest[..end].parse().unwrap()
}

pub(super) fn text_y(svg: &str, label: &str) -> f32 {
    let needle = format!(">{label}<");
    let at = svg.find(&needle).unwrap_or_else(|| panic!("missing {label}"));
    let head = &svg[..at];
    let y_at = head.rfind("y=\"").unwrap_or_else(|| panic!("no y for {label}"));
    let rest = &head[y_at + 3..];
    let end = rest.find('"').unwrap();
    rest[..end].parse().unwrap()
}

pub(super) fn attr_f32(tag: &str, name: &str) -> Option<f32> {
    let key = format!("{name}=\"");
    let at = tag.find(&key)?;
    let rest = &tag[at + key.len()..];
    let end = rest.find('"')?;
    rest[..end].parse().ok()
}

pub(super) fn painted_rects(svg: &str, fill: &str) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    let mut rest = svg;
    let needle = format!("fill=\"{fill}\"");
    while let Some(at) = rest.find("<rect ") {
        let end = rest[at..].find("/>").map(|i| at + i).unwrap_or(rest.len());
        let tag = &rest[at..end];
        if tag.contains(&needle)
            && let (Some(x), Some(y), Some(w), Some(h)) = (attr_f32(tag, "x"), attr_f32(tag, "y"), attr_f32(tag, "width"), attr_f32(tag, "height"))
        {
            out.push((x, y, w, h));
        }
        rest = &rest[end..];
    }
    out
}

pub(super) fn view_box(svg: &str) -> (f32, f32) {
    let at = svg.find("viewBox=\"").expect("viewBox");
    let rest = &svg[at + 9..];
    let end = rest.find('"').unwrap();
    let parts: Vec<f32> = rest[..end].split_whitespace().map(|part| part.parse().unwrap()).collect();
    (parts[2], parts[3])
}

pub(super) fn rect_holds(rect: (f32, f32, f32, f32), px: f32, py: f32) -> bool {
    px >= rect.0 && py >= rect.1 && px <= rect.0 + rect.2 && py <= rect.1 + rect.3
}

pub(super) fn rects_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 - 0.5 && a.0 + a.2 > b.0 + 0.5 && a.1 < b.1 + b.3 - 0.5 && a.1 + a.3 > b.1 + 0.5
}

pub(super) fn fill_none_paths(svg: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("<path ") {
        let end = rest[at..].find("/>").map(|i| at + i).unwrap_or(rest.len());
        let tag = &rest[at..end];
        if tag.contains("fill=\"none\"")
            && let Some(d_at) = tag.find("d=\"")
        {
            let d_rest = &tag[d_at + 3..];
            let d_end = d_rest.find('"').unwrap();
            out.push((d_rest[..d_end].to_string(), tag.contains("data-route=\"detour\"")));
        }
        rest = &rest[end..];
    }
    out
}

pub(super) fn inside_inset(px: f32, py: f32, rect: (f32, f32, f32, f32)) -> bool {
    px > rect.0 + 1.0 && py > rect.1 + 1.0 && px < rect.0 + rect.2 - 1.0 && py < rect.1 + rect.3 - 1.0
}

pub(super) fn tag_with<'a>(svg: &'a str, marker: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find(marker) {
        let start = rest[..at + 1].rfind('<').unwrap_or(0);
        let end = rest[at..].find('>').map_or(rest.len(), |e| at + e + 1);
        out.push(&rest[start..end]);
        rest = &rest[end..];
    }
    out
}
