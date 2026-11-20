// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The styles of an OpenDocument presentation ([ODF 1.3] part 1, chapters 16 and 20): named and automatic styles with their
//! parents, the default styles, list styles, and the drawing objects styles refer to (gradients, dashes, markers, images).

use std::collections::HashMap;

use roxmltree::Node;

pub(crate) const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
pub(crate) const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
pub(crate) const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
pub(crate) const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
pub(crate) const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
pub(crate) const SVG: &str = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0";
pub(crate) const PRESENTATION: &str = "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0";
pub(crate) const XLINK: &str = "http://www.w3.org/1999/xlink";
pub(crate) const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
pub(crate) const SMIL: &str = "urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0";
pub(crate) const LOEXT: &str = "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0";

pub(crate) fn attr<'a>(n: Node<'a, '_>, ns: &str, local: &str) -> Option<&'a str> {
    n.attribute((ns, local))
}

pub(crate) fn children<'a, 'i>(n: Node<'a, 'i>, ns: &'static str, local: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().namespace() == Some(ns) && c.tag_name().name() == local)
}

pub(crate) fn is(n: Node, ns: &str, local: &str) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(ns) && n.tag_name().name() == local
}

/// A length in EMU: "2.5cm", "10mm", "1in", "12pt", "96px", "1pc" (a bare number is centimetres' hundredth? no: it is invalid and 0).
pub(crate) fn emu(text: &str) -> Option<i64> {
    let t = text.trim();
    let split = t.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let v: f64 = num.trim().parse().ok()?;
    let per = match unit.trim() {
        "cm" => 360_000.0,
        "mm" => 36_000.0,
        "in" => 914_400.0,
        "pt" => 12_700.0,
        "pc" => 152_400.0,
        "px" => 9_525.0,
        "" => 360_000.0,
        _ => return None,
    };
    Some((v * per).round() as i64)
}

/// A length in points.
pub(crate) fn points(text: &str) -> Option<f64> {
    emu(text).map(|v| v as f64 / 12_700.0)
}

/// A percentage ("45%") as a fraction.
pub(crate) fn percent(text: &str) -> Option<f64> {
    text.trim().strip_suffix('%')?.trim().parse::<f64>().ok().map(|v| v / 100.0)
}

/// A colour "#rrggbb" as `RRGGBB`.
pub(crate) fn color(text: &str) -> Option<String> {
    let t = text.trim().strip_prefix('#')?;
    (t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit())).then(|| t.to_ascii_uppercase())
}

/// The styles of a document, from its `styles.xml` and `content.xml`.
pub(crate) struct Styles<'a, 'i> {
    named: HashMap<(String, String), Node<'a, 'i>>,
    defaults: HashMap<String, Node<'a, 'i>>,
    pub(crate) lists: HashMap<String, Node<'a, 'i>>,
    pub(crate) gradients: HashMap<String, Node<'a, 'i>>,
    pub(crate) dashes: HashMap<String, Node<'a, 'i>>,
    pub(crate) markers: HashMap<String, Node<'a, 'i>>,
    pub(crate) images: HashMap<String, Node<'a, 'i>>,
    pub(crate) hatches: HashMap<String, Node<'a, 'i>>,
    pub(crate) fonts: HashMap<String, String>,
    pub(crate) page_layouts: HashMap<String, Node<'a, 'i>>,
    pub(crate) master_pages: Vec<Node<'a, 'i>>,
}

impl<'a, 'i> Styles<'a, 'i> {
    /// Collect the styles of the document roots (`styles.xml` first, then `content.xml`, so that automatic styles win).
    pub(crate) fn new(roots: &[Node<'a, 'i>]) -> Self {
        let mut s = Styles {
            named: HashMap::new(),
            defaults: HashMap::new(),
            lists: HashMap::new(),
            gradients: HashMap::new(),
            dashes: HashMap::new(),
            markers: HashMap::new(),
            images: HashMap::new(),
            hatches: HashMap::new(),
            fonts: HashMap::new(),
            page_layouts: HashMap::new(),
            master_pages: Vec::new(),
        };
        for root in roots {
            for holder in root.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(OFFICE)) {
                match holder.tag_name().name() {
                    "font-face-decls" => {
                        for f in holder.children().filter(|c| is(*c, STYLE, "font-face")) {
                            if let Some(name) = attr(f, STYLE, "name") {
                                let family = attr(f, SVG, "font-family").map(|v| v.trim_matches('\'').to_string()).unwrap_or_else(|| name.to_string());
                                s.fonts.insert(name.to_string(), family);
                            }
                        }
                    }
                    "styles" | "automatic-styles" => {
                        for st in holder.children().filter(|c| c.is_element()) {
                            s.add(st);
                        }
                    }
                    "master-styles" => {
                        for st in holder.children().filter(|c| is(*c, STYLE, "master-page")) {
                            s.master_pages.push(st);
                        }
                    }
                    _ => {}
                }
            }
        }
        s
    }

    fn add(&mut self, st: Node<'a, 'i>) {
        let ns = st.tag_name().namespace();
        let name = attr(st, STYLE, "name").or_else(|| attr(st, DRAW, "name")).map(str::to_string);
        match (ns, st.tag_name().name()) {
            (Some(STYLE), "style") => {
                if let (Some(name), Some(family)) = (name, attr(st, STYLE, "family")) {
                    self.named.insert((family.to_string(), name), st);
                }
            }
            (Some(STYLE), "default-style") => {
                if let Some(family) = attr(st, STYLE, "family") {
                    self.defaults.insert(family.to_string(), st);
                }
            }
            (Some(TEXT), "list-style") => {
                if let Some(name) = name {
                    self.lists.insert(name, st);
                }
            }
            (Some(DRAW), "gradient") | (Some(LOEXT), "gradient") => {
                if let Some(name) = name {
                    self.gradients.insert(name, st);
                }
            }
            (Some(DRAW), "stroke-dash") => {
                if let Some(name) = name {
                    self.dashes.insert(name, st);
                }
            }
            (Some(DRAW), "marker") => {
                if let Some(name) = name {
                    self.markers.insert(name, st);
                }
            }
            (Some(DRAW), "fill-image") => {
                if let Some(name) = name {
                    self.images.insert(name, st);
                }
            }
            (Some(DRAW), "hatch") => {
                if let Some(name) = name {
                    self.hatches.insert(name, st);
                }
            }
            (Some(STYLE), "page-layout") => {
                if let Some(name) = name {
                    self.page_layouts.insert(name, st);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn style(&self, family: &str, name: &str) -> Option<Node<'a, 'i>> {
        self.named.get(&(family.to_string(), name.to_string())).copied()
    }

    /// The style and its parents, nearest first (the family's default style last).
    pub(crate) fn chain(&self, family: &str, name: &str) -> Vec<Node<'a, 'i>> {
        let mut out = Vec::new();
        let mut at = self.style(family, name);
        while let Some(node) = at {
            if out.len() > 16 {
                break;
            }
            out.push(node);
            at = attr(node, STYLE, "parent-style-name").and_then(|p| self.style(family, p));
        }
        out
    }

    pub(crate) fn default_style(&self, family: &str) -> Option<Node<'a, 'i>> {
        self.defaults.get(family).copied()
    }

    /// The value of the attribute in the style `name` of `family`, its parents or the family's default style.
    pub(crate) fn prop(&self, family: &str, name: &str, ns: &str, local: &str) -> Option<&'a str> {
        for node in self.chain(family, name) {
            if let Some(v) = node_prop(node, ns, local) {
                return Some(v);
            }
        }
        self.default_style(family).and_then(|d| node_prop(d, ns, local))
    }

    /// The first value found along `names` (family, name), the last being the most specific.
    pub(crate) fn prop_in(&self, names: &[(&str, &str)], ns: &str, local: &str) -> Option<&'a str> {
        for (family, name) in names.iter().rev() {
            for node in self.chain(family, name) {
                if let Some(v) = node_prop(node, ns, local) {
                    return Some(v);
                }
            }
        }
        for (family, _) in names.iter().rev() {
            if let Some(v) = self.default_style(family).and_then(|d| node_prop(d, ns, local)) {
                return Some(v);
            }
        }
        None
    }

    /// The list style a name gives, or the one a style carries inside (a presentation style may hold its own).
    pub(crate) fn list_style(&self, name: &str) -> Option<Node<'a, 'i>> {
        self.lists.get(name).copied()
    }

    /// The font family a font name stands for.
    pub(crate) fn font_family(&self, name: &str) -> String {
        self.fonts.get(name).cloned().unwrap_or_else(|| name.to_string())
    }
}

/// An attribute of any properties element (`style:text-properties`, `style:graphic-properties` ...) in a style.
pub(crate) fn node_prop<'a>(style: Node<'a, '_>, ns: &str, local: &str) -> Option<&'a str> {
    style.children().filter(|c| c.is_element() && c.tag_name().name().ends_with("-properties")).find_map(|p| p.attribute((ns, local)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_are_read_in_their_units() {
        assert_eq!(emu("2.54cm"), Some(914_400));
        assert_eq!(emu("1in"), Some(914_400));
        assert_eq!(emu("10mm"), Some(360_000));
        assert_eq!(emu("12pt"), Some(152_400));
        assert_eq!(points("0.353cm").map(|p| (p * 10.0).round()), Some(100.0));
        assert_eq!(percent("45%"), Some(0.45));
        assert_eq!(color("#99cc00"), Some("99CC00".to_string()));
    }
}
