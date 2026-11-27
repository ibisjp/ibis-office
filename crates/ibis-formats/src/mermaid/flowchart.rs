// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `flowchart` / `graph`.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FlowDir {
    Td,
    Bt,
    Lr,
    Rl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeShape {
    Rect,
    Round,
    Stadium,
    Diamond,
    Circle,
    Subroutine,
    Cylinder,
    Hexagon,
    Trapezoid,
    InvTrapezoid,
    Parallelogram,
    Asymmetric,
    /// Expanded `@{ shape: … }` shapes.
    Ext(ExtShape),
}

/// Shapes reachable only through mermaid's expanded syntax `A@{ shape: name }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExtShape {
    DoubleCircle,
    Text,
    Card,
    LinedRect,
    SmallCircle,
    FramedCircle,
    FilledCircle,
    Fork,
    Hourglass,
    BraceL,
    BraceR,
    Braces,
    Bolt,
    Document,
    LinedDocument,
    Documents,
    TaggedDocument,
    Delay,
    HCylinder,
    LinedCylinder,
    Display,
    Divided,
    Triangle,
    FlipTriangle,
    WindowPane,
    LoopLimit,
    ManualInput,
    Processes,
    PaperTape,
    StoredData,
    CrossCircle,
    TaggedRect,
    DataStore,
    LeanLeft,
}

impl ExtShape {
    /// Markers such as start/stop circles, fork bars and the lightning bolt carry no text in mermaid.
    pub(super) fn hides_label(self) -> bool {
        matches!(
            self,
            ExtShape::SmallCircle | ExtShape::FilledCircle | ExtShape::FramedCircle | ExtShape::Fork | ExtShape::Bolt
        )
    }
}

/// `shape:` names and aliases from mermaid's "Expanded Node Shapes" table.
pub(super) fn flow_shape_by_name(name: &str) -> Option<NodeShape> {
    use ExtShape as E;
    let ext = |e: ExtShape| Some(NodeShape::Ext(e));
    match name.trim().to_ascii_lowercase().as_str() {
        "rect" | "rectangle" | "proc" | "process" | "square" => Some(NodeShape::Rect),
        "rounded" | "event" => Some(NodeShape::Round),
        "stadium" | "pill" | "terminal" => Some(NodeShape::Stadium),
        "fr-rect" | "subproc" | "subprocess" | "framed-rectangle" | "subroutine" => Some(NodeShape::Subroutine),
        "cyl" | "cylinder" | "database" | "db" => Some(NodeShape::Cylinder),
        "circle" | "circ" => Some(NodeShape::Circle),
        "odd" => Some(NodeShape::Asymmetric),
        "diam" | "diamond" | "decision" | "question" => Some(NodeShape::Diamond),
        "hex" | "hexagon" | "prepare" => Some(NodeShape::Hexagon),
        "lean-r" | "lean-right" | "in-out" => Some(NodeShape::Parallelogram),
        "lean-l" | "lean-left" | "out-in" => ext(E::LeanLeft),
        "trap-b" | "trapezoid-bottom" | "priority" | "trapezoid" => Some(NodeShape::Trapezoid),
        "trap-t" | "trapezoid-top" | "manual" | "inv-trapezoid" => Some(NodeShape::InvTrapezoid),
        "dbl-circ" | "double-circle" => ext(E::DoubleCircle),
        "text" => ext(E::Text),
        "notch-rect" | "card" | "notched-rectangle" => ext(E::Card),
        "lin-rect" | "lin-proc" | "lined-rectangle" | "lined-process" | "shaded-process" => ext(E::LinedRect),
        "sm-circ" | "small-circle" | "start" => ext(E::SmallCircle),
        "fr-circ" | "framed-circle" | "stop" => ext(E::FramedCircle),
        "f-circ" | "filled-circle" | "junction" => ext(E::FilledCircle),
        "fork" | "join" => ext(E::Fork),
        "hourglass" | "collate" => ext(E::Hourglass),
        "comment" | "brace" | "brace-l" => ext(E::BraceL),
        "brace-r" => ext(E::BraceR),
        "braces" => ext(E::Braces),
        "bolt" | "com-link" | "lightning-bolt" => ext(E::Bolt),
        "doc" | "document" => ext(E::Document),
        "lin-doc" | "lined-document" => ext(E::LinedDocument),
        "docs" | "documents" | "st-doc" | "stacked-document" => ext(E::Documents),
        "tag-doc" | "tagged-document" => ext(E::TaggedDocument),
        "delay" | "half-rounded-rectangle" => ext(E::Delay),
        "das" | "h-cyl" | "horizontal-cylinder" => ext(E::HCylinder),
        "lin-cyl" | "disk" | "lined-cylinder" => ext(E::LinedCylinder),
        "curv-trap" | "curved-trapezoid" | "display" => ext(E::Display),
        "div-rect" | "div-proc" | "divided-rectangle" | "divided-process" => ext(E::Divided),
        "tri" | "triangle" | "extract" => ext(E::Triangle),
        "flip-tri" | "flipped-triangle" | "manual-file" => ext(E::FlipTriangle),
        "win-pane" | "window-pane" | "internal-storage" => ext(E::WindowPane),
        "notch-pent" | "notched-pentagon" | "loop-limit" => ext(E::LoopLimit),
        "sl-rect" | "sloped-rectangle" | "manual-input" => ext(E::ManualInput),
        "procs" | "processes" | "st-rect" | "stacked-rectangle" => ext(E::Processes),
        "flag" | "paper-tape" => ext(E::PaperTape),
        "bow-rect" | "bow-tie-rectangle" | "stored-data" => ext(E::StoredData),
        "cross-circ" | "crossed-circle" | "summary" => ext(E::CrossCircle),
        "tag-rect" | "tag-proc" | "tagged-rectangle" | "tagged-process" => ext(E::TaggedRect),
        "datastore" | "data-store" => ext(E::DataStore),
        _ => None,
    }
}

/// `shape: cyl, label: "Database"` → key/value pairs (commas and colons inside quotes stay).
pub(super) fn parse_brace_attrs(inner: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = inner.trim();
    while !rest.is_empty() {
        let end = find_unquoted(rest, ',').unwrap_or(rest.len());
        let piece = &rest[..end];
        if let Some(colon) = find_unquoted(piece, ':') {
            let key = unquote(&piece[..colon]);
            let value = unquote(&piece[colon + 1..]);
            if !key.is_empty() {
                out.push((key, value));
            }
        }
        rest = rest[(end + 1).min(rest.len())..].trim_start();
    }
    out
}

/// `ID@{ … }` as a whole statement: (`ID`, attributes).
pub(super) fn split_attr_statement(stmt: &str) -> Option<(&str, Vec<(String, String)>)> {
    let at = stmt.find("@{")?;
    let id = stmt[..at].trim();
    if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.')) || !stmt.ends_with('}') {
        return None;
    }
    Some((id, parse_brace_attrs(&stmt[at + 2..stmt.len() - 1])))
}

pub(super) fn flow_ext_size(shape: ExtShape, tw: f32, text_h: f32, horizontal: bool) -> (f32, f32) {
    use ExtShape as E;
    let base = ((tw + 36.0).max(84.0), (text_h + 18.0).max(40.0));
    match shape {
        E::SmallCircle => (16.0, 16.0),
        E::FilledCircle => (12.0, 12.0),
        E::FramedCircle => (22.0, 22.0),
        E::Fork if horizontal => (10.0, 64.0),
        E::Fork => (64.0, 10.0),
        E::Bolt => (28.0, 40.0),
        E::Text => ((tw + 12.0).max(24.0), text_h + 8.0),
        E::DoubleCircle | E::CrossCircle => {
            let d = (tw + 36.0).max(text_h + 30.0).max(60.0);
            (d, d)
        }
        E::Triangle | E::FlipTriangle => (((tw + 24.0) * 2.0).max(96.0), (text_h * 2.0 + 30.0).max(64.0)),
        E::Hourglass => ((tw + 24.0).max(64.0), (text_h + 40.0).max(64.0)),
        E::Document | E::LinedDocument | E::TaggedDocument => (base.0, base.1 + 10.0),
        E::Documents | E::Processes => (base.0 + 10.0, base.1 + 14.0),
        E::Delay | E::Display | E::StoredData | E::HCylinder => (base.0 + 14.0, base.1),
        E::LinedCylinder => (base.0, (text_h + 56.0).max(68.0)),
        E::Divided | E::WindowPane | E::ManualInput | E::LoopLimit | E::PaperTape => (base.0, base.1 + 10.0),
        E::BraceL | E::BraceR | E::Braces => ((tw + 28.0).max(60.0), base.1),
        _ => base,
    }
}

/// Draw an expanded shape inside the box `x, y, w, h`. Returns how far the label moves down
/// (triangles put their text in the wide half).
pub(super) fn push_ext_shape(svg: &mut String, shape: ExtShape, x: f32, y: f32, w: f32, h: f32) -> f32 {
    use ExtShape as E;
    const F: &str = "#eff6ff";
    const S: &str = "#2563eb";
    const INK: &str = "#0f172a";
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let rect = |svg: &mut String, x: f32, y: f32, w: f32, h: f32, name: &str| {
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="2" fill="{F}" stroke="{S}" stroke-width="1.4" data-shape="{name}"/>"##
        ));
    };
    let line = |svg: &mut String, x1: f32, y1: f32, x2: f32, y2: f32| {
        svg.push_str(&format!(
            r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="{S}" stroke-width="1.2"/>"##
        ));
    };
    let path = |svg: &mut String, d: String, fill: &str, name: &str| {
        svg.push_str(&format!(
            r##"<path d="{d}" fill="{fill}" stroke="{S}" stroke-width="1.4" data-shape="{name}"/>"##
        ));
    };
    let wave = 6.0f32;
    let doc_d = |x: f32, y: f32, w: f32, h: f32| {
        format!(
            "M {x:.1} {y:.1} L {:.1} {y:.1} L {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {x:.1} {:.1} Z",
            x + w,
            x + w,
            y + h - wave,
            x + w * 0.7,
            y + h - wave * 3.0,
            x + w * 0.3,
            y + h + wave,
            y + h - wave
        )
    };
    match shape {
        E::DoubleCircle => {
            let r = w.min(h) / 2.0;
            svg.push_str(&format!(
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="{F}" stroke="{S}" stroke-width="1.4" data-shape="double-circle"/>"##
            ));
            svg.push_str(&format!(
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}" fill="none" stroke="{S}" stroke-width="1.4"/>"##,
                r - 4.0
            ));
        }
        E::CrossCircle => {
            let r = w.min(h) / 2.0;
            let d = r * std::f32::consts::FRAC_1_SQRT_2;
            svg.push_str(&format!(
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="{F}" stroke="{S}" stroke-width="1.4" data-shape="cross-circle"/>"##
            ));
            line(svg, cx - d, cy - d, cx + d, cy + d);
            line(svg, cx + d, cy - d, cx - d, cy + d);
        }
        E::Text => svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="none" stroke="none" data-shape="text"/>"##
        )),
        E::Card => push_flow_polygon(
            svg,
            &[(x + 12.0, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y + 12.0)],
            F,
            S,
            "card",
        ),
        E::LinedRect => {
            rect(svg, x, y, w, h, "lined-rect");
            line(svg, x + 8.0, y, x + 8.0, y + h);
        }
        E::SmallCircle | E::FilledCircle => svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}" fill="{INK}" stroke="{INK}" stroke-width="1" data-shape="{}"/>"##,
            w.min(h) / 2.0,
            if shape == E::SmallCircle { "small-circle" } else { "filled-circle" }
        )),
        E::FramedCircle => {
            let r = w.min(h) / 2.0;
            svg.push_str(&format!(
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="#ffffff" stroke="{INK}" stroke-width="1.4" data-shape="framed-circle"/>"##
            ));
            svg.push_str(&format!(r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}" fill="{INK}"/>"##, r - 4.0));
        }
        E::Fork => svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="2" fill="{INK}" stroke="{INK}" stroke-width="1" data-shape="fork"/>"##
        )),
        E::Hourglass => push_flow_polygon(svg, &[(x, y), (x + w, y), (x, y + h), (x + w, y + h)], F, S, "hourglass"),
        E::BraceL | E::BraceR | E::Braces => {
            let brace = |left: bool| {
                let (edge, tip, bend) = if left { (x + 4.0, x - 2.0, x + 10.0) } else { (x + w - 4.0, x + w + 2.0, x + w - 10.0) };
                format!(
                    "M {bend:.1} {y:.1} Q {edge:.1} {y:.1} {edge:.1} {:.1} L {edge:.1} {:.1} Q {edge:.1} {cy:.1} {tip:.1} {cy:.1} Q {edge:.1} {cy:.1} {edge:.1} {:.1} L {edge:.1} {:.1} Q {edge:.1} {:.1} {bend:.1} {:.1}",
                    y + 6.0,
                    cy - 6.0,
                    cy + 6.0,
                    y + h - 6.0,
                    y + h,
                    y + h
                )
            };
            if shape != E::BraceR {
                path(svg, brace(true), "none", "brace-l");
            }
            if shape != E::BraceL {
                path(svg, brace(false), "none", "brace-r");
            }
        }
        E::Bolt => push_flow_polygon(
            svg,
            &[
                (x + w * 0.62, y),
                (x + w * 0.12, y + h * 0.56),
                (x + w * 0.46, y + h * 0.56),
                (x + w * 0.32, y + h),
                (x + w * 0.88, y + h * 0.4),
                (x + w * 0.52, y + h * 0.4),
            ],
            F,
            S,
            "bolt",
        ),
        E::Document => path(svg, doc_d(x, y, w, h), F, "document"),
        E::LinedDocument => {
            path(svg, doc_d(x, y, w, h), F, "lined-document");
            line(svg, x + 8.0, y, x + 8.0, y + h - wave);
        }
        E::TaggedDocument => {
            path(svg, doc_d(x, y, w, h), F, "tagged-document");
            push_flow_polygon(
                svg,
                &[(x + w, y + h - wave - 16.0), (x + w, y + h - wave), (x + w - 16.0, y + h - wave + 1.5)],
                S,
                S,
                "tag",
            );
        }
        E::Documents => {
            for (i, name) in [(2.0f32, "documents-back"), (1.0, "documents-middle"), (0.0, "documents")] {
                path(svg, doc_d(x + i * 5.0, y + (2.0 - i) * 5.0, w - 10.0, h - 10.0), F, name);
            }
        }
        E::Processes => {
            for (i, name) in [(2.0f32, "processes-back"), (1.0, "processes-middle"), (0.0, "processes")] {
                rect(svg, x + i * 5.0, y + (2.0 - i) * 5.0, w - 10.0, h - 10.0, name);
            }
        }
        E::Delay => {
            let r = (h / 2.0).min(w / 2.0);
            path(
                svg,
                format!(
                    "M {x:.1} {y:.1} L {:.1} {y:.1} A {r:.1} {r:.1} 0 0 1 {:.1} {:.1} L {x:.1} {:.1} Z",
                    x + w - r,
                    x + w - r,
                    y + h,
                    y + h
                ),
                F,
                "delay",
            );
        }
        E::HCylinder => {
            let r = (w / 6.0).clamp(6.0, 12.0);
            path(
                svg,
                format!(
                    "M {:.1} {y:.1} L {:.1} {y:.1} A {r:.1} {:.1} 0 0 1 {:.1} {:.1} L {:.1} {:.1} A {r:.1} {:.1} 0 0 1 {:.1} {y:.1} Z",
                    x + r,
                    x + w - r,
                    h / 2.0,
                    x + w - r,
                    y + h,
                    x + r,
                    y + h,
                    h / 2.0,
                    x + r
                ),
                F,
                "h-cylinder",
            );
            path(
                svg,
                format!("M {:.1} {y:.1} A {r:.1} {:.1} 0 0 0 {:.1} {:.1}", x + w - r, h / 2.0, x + w - r, y + h),
                "none",
                "h-cylinder-rim",
            );
        }
        E::LinedCylinder => {
            push_flow_cylinder(svg, x, y, w, h);
            let ry = (h * 0.18).clamp(6.0, 14.0);
            path(
                svg,
                format!("M {x:.1} {:.1} A {:.1} {ry:.1} 0 0 0 {:.1} {:.1}", y + ry + 7.0, w / 2.0, x + w, y + ry + 7.0),
                "none",
                "lined-cylinder",
            );
        }
        E::Display => {
            let r = (h / 2.0).min(w / 3.0);
            path(
                svg,
                format!(
                    "M {:.1} {y:.1} L {:.1} {y:.1} A {r:.1} {:.1} 0 0 1 {:.1} {:.1} L {:.1} {:.1} L {x:.1} {cy:.1} Z",
                    x + w * 0.16,
                    x + w - r,
                    h / 2.0,
                    x + w - r,
                    y + h,
                    x + w * 0.16,
                    y + h
                ),
                F,
                "display",
            );
        }
        E::Divided => {
            rect(svg, x, y, w, h, "divided-rect");
            line(svg, x, y + 10.0, x + w, y + 10.0);
            return 5.0;
        }
        E::WindowPane => {
            rect(svg, x, y, w, h, "window-pane");
            line(svg, x + 10.0, y, x + 10.0, y + h);
            line(svg, x, y + 10.0, x + w, y + 10.0);
            return 5.0;
        }
        E::Triangle => {
            push_flow_polygon(svg, &[(cx, y), (x + w, y + h), (x, y + h)], F, S, "triangle");
            return h * 0.2;
        }
        E::FlipTriangle => {
            push_flow_polygon(svg, &[(x, y), (x + w, y), (cx, y + h)], F, S, "flipped-triangle");
            return -h * 0.2;
        }
        E::LoopLimit => push_flow_polygon(
            svg,
            &[(x + 12.0, y), (x + w - 12.0, y), (x + w, y + 12.0), (x + w, y + h), (x, y + h), (x, y + 12.0)],
            F,
            S,
            "loop-limit",
        ),
        E::ManualInput => {
            push_flow_polygon(svg, &[(x, y + 12.0), (x + w, y), (x + w, y + h), (x, y + h)], F, S, "manual-input");
            return 5.0;
        }
        E::PaperTape => {
            let a = 5.0f32;
            path(
                svg,
                format!(
                    "M {x:.1} {:.1} Q {:.1} {:.1} {cx:.1} {:.1} T {:.1} {:.1} L {:.1} {:.1} Q {:.1} {:.1} {cx:.1} {:.1} T {x:.1} {:.1} Z",
                    y + a,
                    x + w / 4.0,
                    y - a,
                    y + a,
                    x + w,
                    y + a,
                    x + w,
                    y + h - a,
                    x + w * 0.75,
                    y + h + a,
                    y + h - a,
                    y + h - a
                ),
                F,
                "paper-tape",
            );
        }
        E::StoredData => {
            let r = 10.0f32;
            path(
                svg,
                format!(
                    "M {:.1} {y:.1} L {:.1} {y:.1} Q {:.1} {cy:.1} {:.1} {:.1} L {:.1} {:.1} Q {:.1} {cy:.1} {:.1} {y:.1} Z",
                    x + r,
                    x + w,
                    x + w - r * 1.6,
                    x + w,
                    y + h,
                    x + r,
                    y + h,
                    x - r * 0.6,
                    x + r
                ),
                F,
                "stored-data",
            );
        }
        E::TaggedRect => {
            rect(svg, x, y, w, h, "tagged-rect");
            push_flow_polygon(svg, &[(x + w, y + h - 14.0), (x + w, y + h), (x + w - 14.0, y + h)], S, S, "tag");
        }
        E::DataStore => {
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="{F}" stroke="none" data-shape="datastore"/>"##
            ));
            line(svg, x, y, x + w, y);
            line(svg, x, y + h, x + w, y + h);
        }
        E::LeanLeft => {
            let skew = w * 0.16;
            push_flow_polygon(svg, &[(x, y), (x + w - skew, y), (x + w, y + h), (x + skew, y + h)], F, S, "lean-left");
        }
    }
    0.0
}

#[derive(Debug, Clone)]
pub(super) struct FlowNode {
    pub(super) _id: String,
    pub(super) label: String,
    pub(super) shape: NodeShape,
    pub(super) subgraph: Option<usize>,
    /// `class id name` / `id:::name`, in order.
    pub(super) classes: Vec<String>,
    /// `@{ icon: … }` / `@{ img: … }`.
    pub(super) media: Option<FlowMedia>,
    /// A subgraph drawn closed (`id@{ view: collapsed }`).
    pub(super) collapsed: bool,
}

/// What an `@{ icon }` / `@{ img }` node shows above (or below) its label.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FlowMediaKind {
    /// `fa:user` and the like: a symbol (there is no icon font).
    Icon(String),
    /// An image URL. Nothing is fetched: a placeholder frame keeps the place.
    Image(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct FlowMedia {
    pub(super) kind: FlowMediaKind,
    /// `pos: t`: the label above the picture (default below).
    pub(super) label_on_top: bool,
    pub(super) w: f32,
    pub(super) h: f32,
    /// `form: square / circle / rounded` (icons).
    pub(super) form: String,
}

/// `A@{ icon: "fa:user", form: "square", label: "User", pos: "t", h: 60 }`.
pub(super) fn parse_node_media(token: &str) -> Option<FlowMedia> {
    let t = token.trim();
    let at = t.find("@{")?;
    if !t.ends_with('}') {
        return None;
    }
    let attrs = parse_brace_attrs(&t[at + 2..t.len() - 1]);
    let get = |key: &str| attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let kind = if let Some(icon) = get("icon") {
        FlowMediaKind::Icon(icon.to_string())
    } else {
        FlowMediaKind::Image(get("img")?.to_string())
    };
    let num = |key: &str| get(key).and_then(|v| v.trim_end_matches("px").parse::<f32>().ok()).map(|v| v.clamp(16.0, 400.0));
    let h = num("h").unwrap_or(48.0);
    let w = num("w").unwrap_or(h);
    Some(FlowMedia {
        kind,
        label_on_top: get("pos").is_some_and(|p| p.eq_ignore_ascii_case("t")),
        w,
        h,
        form: get("form").unwrap_or("").to_ascii_lowercase(),
    })
}

/// A Font Awesome name as a Unicode symbol most fonts have; unknown names get a diamond.
pub(super) fn fa_symbol(name: &str) -> &'static str {
    let name = name.trim_start_matches("fa-");
    let name = name.strip_suffix("-bold").unwrap_or(name);
    match name {
        "ban" | "circle-xmark" => "\u{2298}",
        "spinner" | "rotate" | "sync" | "refresh" => "\u{21BB}",
        "camera" | "camera-retro" => "\u{25C9}",
        "user" | "person" | "user-circle" => "\u{263A}",
        "users" => "\u{263A}\u{263A}",
        "home" | "house" => "\u{2302}",
        "envelope" | "mail" => "\u{2709}",
        "phone" => "\u{260E}",
        "star" => "\u{2605}",
        "heart" => "\u{2665}",
        "cog" | "gear" | "gears" | "cogs" => "\u{2699}",
        "check" | "circle-check" => "\u{2714}",
        "times" | "xmark" | "close" => "\u{2716}",
        "plane" => "\u{2708}",
        "cloud" => "\u{2601}",
        "warning" | "exclamation-triangle" | "triangle-exclamation" => "\u{26A0}",
        "info" | "info-circle" | "circle-info" => "\u{2139}",
        "bolt" => "\u{26A1}",
        "clock" => "\u{25F7}",
        "music" => "\u{266B}",
        "flag" => "\u{2691}",
        "search" | "magnifying-glass" => "\u{2315}",
        "pencil" | "edit" | "pen" => "\u{270E}",
        "twitter" | "x-twitter" => "\u{2726}",
        _ => "\u{25C6}",
    }
}

/// `fa:fa-ban forbidden` → `forbidden`; `fab:` / `fas:` / `far:` / `fal:` / `fak:` too.
pub(super) fn replace_fa_icons(text: &str) -> String {
    if !text.contains(":fa-") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(":fa-") {
        // The prefix is `fa` plus at most one letter, starting a word.
        let head = &rest[..at];
        let prefix_len = ["fab", "fas", "far", "fal", "fak", "fa"].iter().find(|p| head.ends_with(**p)).map(|p| p.len());
        let Some(prefix_len) = prefix_len.filter(|&n| {
            let before = &head[..head.len() - n];
            before.is_empty() || before.ends_with(|c: char| !c.is_alphanumeric())
        }) else {
            out.push_str(&rest[..at + 4]);
            rest = &rest[at + 4..];
            continue;
        };
        out.push_str(&head[..head.len() - prefix_len]);
        let name_start = at + 1;
        let name_end = rest[name_start..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).map_or(rest.len(), |e| name_start + e);
        // mermaid.js draws the icon as a graphic and keeps the surrounding words as the label
        // (`A fa:fa-camera-retro perhaps?` -> `A perhaps?`).
        rest = &rest[name_end..].trim_start();
        if !out.is_empty() && !out.ends_with(' ') && !rest.is_empty() {
            out.push(' ');
        }
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EdgeStyle {
    Solid,
    Dashed,
    Thick,
    /// `~~~`: takes part in the layout but is not drawn.
    Invisible,
}

/// What an edge ends in: `-->` (arrow), `--o` (circle), `--x` (cross) or nothing (`---`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EdgeEnd {
    None,
    Arrow,
    Circle,
    Cross,
}

/// One parsed link token between two node groups.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct EdgeTok {
    pub(super) style: EdgeStyle,
    pub(super) start: EdgeEnd,
    pub(super) end: EdgeEnd,
    pub(super) label: Option<String>,
    /// `A e1@--> B`: the id `e1@{ … }` statements refer to.
    pub(super) id: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct FlowEdge {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) label: Option<String>,
    pub(super) style: EdgeStyle,
    pub(super) start: EdgeEnd,
    pub(super) end: EdgeEnd,
    pub(super) id: Option<String>,
    /// `e1@{ curve: linear }` (also `step*`): drawn straight instead of curved.
    pub(super) straight: bool,
}

/// The subset of CSS properties `classDef`, `class`, `style` and `linkStyle` apply.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct FlowStyle {
    pub(super) fill: Option<String>,
    pub(super) stroke: Option<String>,
    pub(super) stroke_width: Option<String>,
    pub(super) dasharray: Option<String>,
    pub(super) color: Option<String>,
    /// `font-weight` / `font-style` of the label (used by state diagrams).
    pub(super) font_weight: Option<String>,
    pub(super) font_style: Option<String>,
}

impl FlowStyle {
    pub(super) fn merge(&mut self, other: &FlowStyle) {
        if other.fill.is_some() {
            self.fill.clone_from(&other.fill);
        }
        if other.stroke.is_some() {
            self.stroke.clone_from(&other.stroke);
        }
        if other.stroke_width.is_some() {
            self.stroke_width.clone_from(&other.stroke_width);
        }
        if other.dasharray.is_some() {
            self.dasharray.clone_from(&other.dasharray);
        }
        if other.color.is_some() {
            self.color.clone_from(&other.color);
        }
        if other.font_weight.is_some() {
            self.font_weight.clone_from(&other.font_weight);
        }
        if other.font_style.is_some() {
            self.font_style.clone_from(&other.font_style);
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        *self == FlowStyle::default()
    }
}

/// `fill:#f9f,stroke:#333,stroke-width:4px` (commas inside `rgb(...)` stay). Unknown keys and
/// values with characters outside a conservative CSS set are dropped, so nothing reaches the SVG unescaped.
pub(super) fn parse_flow_style(props: &str) -> FlowStyle {
    let mut out = FlowStyle::default();
    let mut depth = 0i32;
    let mut start = 0usize;
    let props = props.trim().trim_end_matches(';');
    let mut pieces = Vec::new();
    for (idx, ch) in props.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' | ';' if depth == 0 => {
                pieces.push(&props[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    pieces.push(&props[start..]);
    for piece in pieces {
        let Some((key, value)) = piece.split_once(':') else { continue };
        let value = value.trim().trim_end_matches("!important").trim();
        if value.is_empty()
            || !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | '.' | '%' | '(' | ')' | ',' | ' ' | '-'))
        {
            continue;
        }
        let value = value.to_string();
        match key.trim() {
            "fill" | "background" | "background-color" => out.fill = Some(value),
            "stroke" => out.stroke = Some(value),
            "stroke-width" => out.stroke_width = Some(value.trim_end_matches("px").trim().to_string()),
            "stroke-dasharray" => out.dasharray = Some(value),
            "color" => out.color = Some(value),
            "font-weight" => out.font_weight = Some(value),
            "font-style" => out.font_style = Some(value),
            _ => {}
        }
    }
    out
}

/// Rewrite the paint attributes of a fragment drawn by this module: every `fill` that is not
/// `none`, every `stroke` and `stroke-width`, and a dash pattern on every element.
pub(super) fn restyle_svg_fragment(fragment: &str, style: &FlowStyle) -> String {
    fn replace_attr(input: &str, name: &str, value: &str, skip_none: bool) -> String {
        let needle = format!(" {name}=\"");
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(at) = rest.find(&needle) {
            let value_start = at + needle.len();
            let Some(len) = rest[value_start..].find('"') else { break };
            let old = &rest[value_start..value_start + len];
            out.push_str(&rest[..value_start]);
            if skip_none && old == "none" {
                out.push_str(old);
            } else {
                out.push_str(&escape_attr(value));
            }
            rest = &rest[value_start + len..];
        }
        out.push_str(rest);
        out
    }
    let mut out = fragment.to_string();
    if let Some(fill) = &style.fill {
        out = replace_attr(&out, "fill", fill, true);
    }
    if let Some(stroke) = &style.stroke {
        out = replace_attr(&out, "stroke", stroke, true);
    }
    if let Some(width) = &style.stroke_width {
        out = replace_attr(&out, "stroke-width", width, false);
    }
    if let Some(dash) = &style.dasharray {
        out = out.replace("/>", &format!(r#" stroke-dasharray="{}"/>"#, escape_attr(dash)));
    }
    out
}

#[derive(Debug, Clone)]
pub(super) struct SubgraphInfo {
    /// The id edges use (`subgraph one [My Group]` → `one`).
    pub(super) id: String,
    pub(super) title: String,
    pub(super) dir: Option<FlowDir>,
    /// The enclosing subgraph.
    pub(super) parent: Option<usize>,
}

pub(super) const FLOW_NODE_GAP: f32 = 34.0;
pub(super) const FLOW_RANK_GAP: f32 = 64.0;

pub(super) fn render_flowchart_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    render_flowchart_svg_with_spacing(source, FLOW_NODE_GAP, FLOW_RANK_GAP)
}

pub(super) fn render_flowchart_svg_with_spacing(
    source: &str,
    node_gap: f32,
    rank_gap: f32,
) -> Option<(Vec<u8>, u32, u32)> {
    let mut lines = source.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let header = lines.next()?;
    let dir_token = header
        .split_whitespace()
        .nth(1)
        .unwrap_or("TD")
        .trim_end_matches(';')
        .to_ascii_uppercase();
    let dir = match dir_token.as_str() {
        "LR" => FlowDir::Lr,
        "RL" => FlowDir::Rl,
        "BT" => FlowDir::Bt,
        _ => FlowDir::Td,
    };

    let mut nodes: Vec<FlowNode> = Vec::new();
    let mut node_map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<FlowEdge> = Vec::new();
    let mut subgraphs: Vec<SubgraphInfo> = Vec::new();
    let mut sg_stack: Vec<usize> = Vec::new();
    let mut collapsed_ids: Vec<String> = Vec::new();
    let mut class_defs: HashMap<String, FlowStyle> = HashMap::new();
    let mut class_assign: Vec<(String, String)> = Vec::new();
    let mut node_styles: Vec<(String, FlowStyle)> = Vec::new();
    let mut link_styles: Vec<(Option<usize>, FlowStyle)> = Vec::new();

    for line in lines {
        for stmt in split_flow_statements(line) {
            let l = stmt.trim();
            if l.is_empty() || l.starts_with("click ") || l.starts_with("accTitle") || l.starts_with("accDescr") {
                continue;
            }
            if let Some(rest) = l.strip_prefix("classDef ") {
                let rest = rest.trim();
                let (names, props) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                let style = parse_flow_style(props);
                for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                    class_defs.entry(name.to_string()).or_default().merge(&style);
                }
                continue;
            }
            if let Some(rest) = l.strip_prefix("class ") {
                let rest = rest.trim();
                if let Some((ids, class)) = rest.rsplit_once(char::is_whitespace) {
                    for id in ids.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                        class_assign.push((id.to_string(), class.trim().to_string()));
                    }
                }
                continue;
            }
            if let Some(rest) = l.strip_prefix("style ") {
                let rest = rest.trim();
                if let Some((id, props)) = rest.split_once(char::is_whitespace) {
                    node_styles.push((id.to_string(), parse_flow_style(props)));
                }
                continue;
            }
            if let Some(rest) = l.strip_prefix("linkStyle ") {
                let rest = rest.trim();
                if let Some((which, props)) = rest.split_once(char::is_whitespace) {
                    let style = parse_flow_style(props);
                    if which == "default" {
                        link_styles.push((None, style));
                    } else {
                        for index in which.split(',').filter_map(|n| n.trim().parse::<usize>().ok()) {
                            link_styles.push((Some(index), style.clone()));
                        }
                    }
                }
                continue;
            }
            if l == "end" {
                sg_stack.pop();
                continue;
            }
            if let Some(rest) = l.strip_prefix("subgraph ") {
                let title = parse_subgraph_title(rest.trim());
                let id = parse_subgraph_id(rest.trim());
                let sg_idx = subgraphs.len();
                subgraphs.push(SubgraphInfo { id, title, dir: None, parent: sg_stack.last().copied() });
                sg_stack.push(sg_idx);
                continue;
            }
            if let Some(rest) = l.strip_prefix("direction ") {
                if let Some(&sg) = sg_stack.last() {
                    subgraphs[sg].dir = Some(parse_flow_dir(rest));
                }
                continue;
            }
            // `one@{ view: collapsed }` closes the subgraph `one`.
            if let Some((id, attrs)) = split_attr_statement(l)
                && subgraphs.iter().any(|sg| sg.id == id)
                && attrs.iter().all(|(k, _)| k == "view")
            {
                if attrs.iter().any(|(_, v)| v.eq_ignore_ascii_case("collapsed")) {
                    collapsed_ids.push(id.to_string());
                }
                continue;
            }
            // `e1@{ curve: linear }` styles an edge declared as `A e1@--> B`; animation is ignored.
            if let Some((id, attrs)) = split_attr_statement(l)
                && edges.iter().any(|e| e.id.as_deref() == Some(id))
            {
                for (key, value) in &attrs {
                    if key == "curve" {
                        let straight = matches!(value.as_str(), "linear" | "step" | "stepBefore" | "stepAfter");
                        for e in edges.iter_mut().filter(|e| e.id.as_deref() == Some(id)) {
                            e.straight = straight;
                        }
                    }
                }
                continue;
            }
            parse_flowchart_statement(
                l,
                sg_stack.last().copied(),
                &mut nodes,
                &mut node_map,
                &mut edges,
            );
        }
    }

    if nodes.is_empty() {
        return None;
    }

    // `fa:fa-ban` icons in labels are dropped (the remaining words stay).
    for node in &mut nodes {
        node.label = replace_fa_icons(&node.label);
    }
    for e in &mut edges {
        if let Some(label) = &mut e.label {
            *label = replace_fa_icons(label);
        }
    }
    let ends = resolve_subgraph_ends(&mut nodes, &mut node_map, &mut edges, &subgraphs, &collapsed_ids);

    for (id, class) in class_assign {
        if let Some(&idx) = node_map.get(&id) {
            nodes[idx].classes.push(class);
        }
    }
    let mut node_style: Vec<FlowStyle> = nodes
        .iter()
        .map(|node| {
            let mut style = class_defs.get("default").cloned().unwrap_or_default();
            for class in &node.classes {
                if let Some(def) = class_defs.get(class) {
                    style.merge(def);
                }
            }
            style
        })
        .collect();
    for (id, style) in &node_styles {
        if let Some(&idx) = node_map.get(id) {
            node_style[idx].merge(style);
        }
    }
    let edge_style: Vec<FlowStyle> = (0..edges.len())
        .map(|index| {
            // `linkStyle default` is the base; numbered statements override it.
            let mut style = FlowStyle::default();
            for (_, s) in link_styles.iter().filter(|(which, _)| which.is_none()) {
                style.merge(s);
            }
            for (_, s) in link_styles.iter().filter(|(which, _)| *which == Some(index)) {
                style.merge(s);
            }
            style
        })
        .collect();

    // Node dimensions. Long labels wrap inside a width cap and the box grows taller.
    let mut node_sizes: Vec<(f32, f32)> = nodes
        .iter()
        .map(|node| match &node.media {
            Some(media) => flow_media_size(media, &node.label),
            None => flow_node_size(node.shape, &node.label, matches!(dir, FlowDir::Lr | FlowDir::Rl)),
        })
        .collect();

    // Layers sit far enough apart for the widest edge label (the tallest one top to bottom).
    let label_span = edges
        .iter()
        .filter_map(|e| e.label.as_deref())
        .map(|l| {
            let (w, h) = flow_edge_label_size(l);
            if matches!(dir, FlowDir::Lr | FlowDir::Rl) { w } else { h }
        })
        .fold(0.0f32, f32::max);
    let layer_gap = if label_span > 0.0 { rank_gap.max(label_span + 24.0) } else { rank_gap };
    // Hidden nodes and subgraph aliases are not laid out; an alias's edges go through a node
    // inside its subgraph, then end on the frame.
    let in_subgraph = |j: usize, sg: usize| {
        let mut g = nodes[j].subgraph;
        while let Some(cur) = g {
            if cur == sg {
                return true;
            }
            g = subgraphs[cur].parent;
        }
        false
    };
    let laid: Vec<usize> = (0..nodes.len()).filter(|&i| ends.alias[i].is_none() && !ends.hidden[i]).collect();
    let mut slot = vec![usize::MAX; nodes.len()];
    for (k, &i) in laid.iter().enumerate() {
        slot[i] = k;
    }
    let through = |i: usize| match ends.alias[i] {
        Some(sg) => laid.iter().copied().find(|&j| in_subgraph(j, sg)).map_or(usize::MAX, |j| slot[j]),
        None => slot[i],
    };
    let edge_pairs: Vec<(usize, usize)> = edges
        .iter()
        .map(|e| (through(e.from), through(e.to)))
        .filter(|&(a, b)| a != usize::MAX && b != usize::MAX)
        .collect();
    let member_of: Vec<Option<usize>> = laid.iter().map(|&i| nodes[i].subgraph).collect();
    let laid_sizes: Vec<(f32, f32)> = laid.iter().map(|&i| node_sizes[i]).collect();
    let (laid_centers, total_w, total_h, frames) = layout_flowchart(&member_of, &edge_pairs, &laid_sizes, &subgraphs, dir, layer_gap, node_gap);
    let mut centers = vec![(-1.0e4f32, -1.0e4f32); nodes.len()];
    for (k, &i) in laid.iter().enumerate() {
        centers[i] = laid_centers[k];
    }
    for (i, alias) in ends.alias.iter().enumerate() {
        if let Some(sg) = alias
            && let Some((fx, fy, fw, fh)) = frames.get(*sg).copied().flatten()
        {
            centers[i] = (fx + fw / 2.0, fy + fh / 2.0);
            node_sizes[i] = (fw, fh);
        }
    }
    for (i, hidden) in ends.hidden.iter().enumerate() {
        if *hidden {
            node_sizes[i] = (0.0, 0.0);
        }
    }

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(
        r##"<defs>
  <marker id="flow-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 0 1 L 10 5 L 0 9 z" fill="#475569"/>
  </marker>
  <marker id="flow-circle" viewBox="0 0 10 10" refX="9.5" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <circle cx="5" cy="5" r="4" fill="#ffffff" stroke="#475569" stroke-width="1.5"/>
  </marker>
  <marker id="flow-cross" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
    <line x1="1" y1="1" x2="9" y2="9" stroke="#475569" stroke-width="1.8"/>
    <line x1="9" y1="1" x2="1" y2="9" stroke="#475569" stroke-width="1.8"/>
  </marker>
</defs>"##,
    );
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));

    // Subgraph frames, outermost first (a subgraph opens before the ones inside it).
    for (sg, frame) in subgraphs.iter().zip(&frames) {
        let Some((sx, sy, sw, sh)) = *frame else { continue };
        svg.push_str(&format!(
            r##"<rect x="{sx:.1}" y="{sy:.1}" width="{sw:.1}" height="{sh:.1}" rx="8" fill="#f8fafc" stroke="#cbd5e1" stroke-width="1.2" stroke-dasharray="5,3"/>"##
        ));
        if !sg.title.is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11.5" font-weight="600" fill="#475569">{}</text>"##,
                sx + 10.0,
                sy + 16.0,
                label_markup(&sg.title, false)
            ));
        }
    }

    // Edges follow the diagram direction. A subgraph's own direction is layout-only.
    let horizontal = matches!(dir, FlowDir::Lr | FlowDir::Rl);
    for (edge_idx, e) in edges.iter().enumerate() {
        if e.style == EdgeStyle::Invisible {
            continue;
        }
        // A subgraph frame around either end is not in the way.
        let mut route_centers = centers.clone();
        for (i, alias) in ends.alias.iter().enumerate() {
            if let Some(sg) = alias
                && i != e.from
                && i != e.to
                && let Some((fx, fy, fw, fh)) = frames.get(*sg).copied().flatten()
                && [e.from, e.to].iter().any(|&j| {
                    let (jx, jy) = centers[j];
                    jx > fx && jx < fx + fw && jy > fy && jy < fy + fh
                })
            {
                route_centers[i] = (-1.0e4, -1.0e4);
            }
        }
        let route = route_layered_edge(
            e.from,
            e.to,
            &route_centers,
            &node_sizes,
            horizontal,
            !e.straight,
            (total_w as f32, total_h as f32),
        );
        let linked = &edge_style[edge_idx];
        let stroke_w = linked.stroke_width.clone().unwrap_or_else(|| {
            match e.style {
                EdgeStyle::Thick => "2.6",
                _ => "1.4",
            }
            .to_string()
        });
        let dash = match (&linked.dasharray, e.style) {
            (Some(dash), _) => format!(r##" stroke-dasharray="{}""##, escape_attr(dash)),
            (None, EdgeStyle::Dashed) => r##" stroke-dasharray="5,4""##.to_string(),
            _ => String::new(),
        };
        let marker_id = |end: EdgeEnd| match end {
            EdgeEnd::Arrow => Some("flow-arrow"),
            EdgeEnd::Circle => Some("flow-circle"),
            EdgeEnd::Cross => Some("flow-cross"),
            EdgeEnd::None => None,
        };
        let mut extra = dash;
        if let Some(id) = marker_id(e.start) {
            extra.push_str(&format!(r##" marker-start="url(#{id})""##));
        }
        if let Some(id) = marker_id(e.end) {
            extra.push_str(&format!(r##" marker-end="url(#{id})""##));
        }
        let stroke = linked.stroke.as_deref().map(escape_attr).unwrap_or_else(|| "#475569".to_string());
        push_routed_edge(&mut svg, &route, &stroke, &escape_attr(&stroke_w), &extra);
        if let Some(lbl) = &e.label {
            let (lx, ly) = route.label_at;
            let (lw, lh) = flow_edge_label_size(lbl);
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="{lh:.1}" rx="3" fill="#ffffff" stroke="#e2e8f0" stroke-width="0.8"/>"##,
                lx - lw / 2.0,
                ly - lh / 2.0
            ));
            push_centered_label(&mut svg, lx, ly, lbl, 11.0, 140.0, "#334155", "400");
        }
    }

    // Nodes
    for (idx, node) in nodes.iter().enumerate() {
        if ends.alias[idx].is_some() || ends.hidden[idx] {
            continue;
        }
        let (cx, cy) = centers[idx];
        let (w, h) = node_sizes[idx];
        let x = cx - w / 2.0;
        let y = cy - h / 2.0;
        if let Some(media) = &node.media {
            push_flow_media(&mut svg, media, &node.label, x, y, w, h, &node_style[idx]);
            continue;
        }
        let shape_at = svg.len();
        let label_dy = push_flow_shape(&mut svg, node.shape, x, y, w, h, cx, cy);
        let style = &node_style[idx];
        if !style.is_empty() {
            let shape_svg = svg.split_off(shape_at);
            svg.push_str(&restyle_svg_fragment(&shape_svg, style));
        }
        if let NodeShape::Ext(ext) = node.shape
            && ext.hides_label()
        {
            continue;
        }
        let label_width = match node.shape {
            NodeShape::Diamond | NodeShape::Hexagon | NodeShape::Asymmetric => w * 0.62,
            NodeShape::Circle | NodeShape::Ext(ExtShape::DoubleCircle | ExtShape::CrossCircle) => w * 0.66,
            NodeShape::Ext(ExtShape::Triangle | ExtShape::FlipTriangle) => w * 0.5,
            NodeShape::Ext(ExtShape::Text) => w + 4.0,
            _ => w - 16.0,
        };
        let text_fill = style.color.as_deref().map(escape_attr).unwrap_or_else(|| "#0f172a".to_string());
        // Size and draw share the 168px cap. A wider box must not reflow the lines.
        push_centered_label(&mut svg, cx, cy + label_dy, &node.label, 13.0, label_width.clamp(24.0, 168.0), &text_fill, "500");
        if node.collapsed {
            // A closed subgraph: a dashed outline and a `+` badge.
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="12" fill="none" stroke="#94a3b8" stroke-width="1" stroke-dasharray="4,3" data-role="collapsed-subgraph"/><rect x="{:.1}" y="{:.1}" width="12" height="12" rx="2" fill="#ffffff" stroke="#64748b" stroke-width="1"/><path d="M {:.1} {:.1} h 6 M {:.1} {:.1} v 6" stroke="#334155" stroke-width="1.2"/>"##,
                x - 4.0,
                y - 4.0,
                w + 8.0,
                h + 8.0,
                x + w - 14.0,
                y + 2.0,
                x + w - 11.0,
                y + 8.0,
                x + w - 8.0,
                y + 5.0
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

/// `one [My Group]` → `one`; `"One"` → `One`; `TOP` → `TOP`.
pub(super) fn parse_subgraph_id(rest: &str) -> String {
    match rest.find('[') {
        Some(open) if open > 0 => rest[..open].trim().to_string(),
        _ => unquote(rest),
    }
}

pub(super) fn parse_subgraph_title(rest: &str) -> String {
    if let Some(open) = rest.find('[')
        && let Some(close) = rest.rfind(']')
        && close > open
    {
        return unquote(rest[open + 1..close].trim());
    }
    unquote(rest)
}

pub(super) fn parse_flowchart_statement(
    stmt: &str,
    subgraph: Option<usize>,
    nodes: &mut Vec<FlowNode>,
    node_map: &mut HashMap<String, usize>,
    edges: &mut Vec<FlowEdge>,
) {
    let Some((lhs, mut pending, mut rest)) = split_next_flowchart_edge(stmt) else {
        let _ = ensure_node_group(stmt, subgraph, nodes, node_map);
        return;
    };
    let mut prev_group = ensure_node_group(lhs, subgraph, nodes, node_map);
    loop {
        let next = split_next_flowchart_edge(rest);
        let node_expr = next.as_ref().map_or(rest, |(next_lhs, _, _)| next_lhs);
        let cur_group = ensure_node_group(node_expr, subgraph, nodes, node_map);
        for &u in &prev_group {
            for &v in &cur_group {
                edges.push(FlowEdge {
                    from: u,
                    to: v,
                    label: pending.label.clone(),
                    style: pending.style,
                    start: pending.start,
                    end: pending.end,
                    id: pending.id.clone(),
                    straight: false,
                });
            }
        }
        prev_group = cur_group;
        match next {
            Some((_, tok, next_rest)) => {
                pending = tok;
                rest = next_rest;
            }
            None => break,
        }
    }
}

/// The text before the first link of `s` (skipping bracketed and quoted labels), the link, and the text after it.
pub(super) fn split_next_flowchart_edge(s: &str) -> Option<(&str, EdgeTok, &str)> {
    let bytes = s.as_bytes();
    let mut cursor = 0usize;
    let mut quote = false;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"' {
            quote = !quote;
            cursor += 1;
            continue;
        }
        if !quote && matches!(bytes[cursor], b'[' | b'(' | b'{') {
            let close = match bytes[cursor] {
                b'[' => b']',
                b'(' => b')',
                _ => b'}',
            };
            if let Some(rel_end) = s[cursor + 1..].find(close as char) {
                cursor += 1 + rel_end + 1;
                continue;
            }
        }
        let word_start = cursor == 0 || bytes[cursor - 1].is_ascii_whitespace();
        if !quote && let Some((mut tok, consumed)) = match_flowchart_edge(&s[cursor..], word_start) {
            let mut lhs = s[..cursor].trim();
            // `A e1@--> B`: an edge id is not part of the node.
            if let Some(before_id) = lhs.strip_suffix('@') {
                let (node, id) = before_id.rsplit_once(char::is_whitespace).unwrap_or((before_id, ""));
                tok.id = (!id.trim().is_empty()).then(|| id.trim().to_string());
                lhs = node.trim();
            }
            return Some((lhs, tok, &s[cursor + consumed..]));
        }
        cursor += s[cursor..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

pub(super) fn edge_end_marker(byte: Option<&u8>) -> Option<EdgeEnd> {
    match byte {
        Some(b'>') => Some(EdgeEnd::Arrow),
        Some(b'o') => Some(EdgeEnd::Circle),
        Some(b'x') => Some(EdgeEnd::Cross),
        _ => None,
    }
}

/// The link body at the start of `s` (after any start marker): its style, end marker and length.
/// Solid `--+[-xo>]`, thick `==+[=xo>]`, dotted `-.+-[xo>]?` (or `.+-[xo>]?` closing a
/// dotted text label), invisible `~~~+`.
pub(super) fn match_edge_body(s: &str) -> Option<(EdgeStyle, EdgeEnd, usize)> {
    let b = s.as_bytes();
    let run = |c: u8, from: usize| b[from..].iter().take_while(|&&x| x == c).count();
    let dotted = |from: usize| -> Option<(EdgeStyle, EdgeEnd, usize)> {
        let dots = run(b'.', from);
        if dots == 0 || b.get(from + dots) != Some(&b'-') {
            return None;
        }
        let at = from + dots + 1;
        match edge_end_marker(b.get(at)) {
            Some(end) => Some((EdgeStyle::Dashed, end, at + 1)),
            None => Some((EdgeStyle::Dashed, EdgeEnd::None, at)),
        }
    };
    match b.first()? {
        b'~' => {
            let n = run(b'~', 0);
            (n >= 3).then_some((EdgeStyle::Invisible, EdgeEnd::None, n))
        }
        b'-' if b.get(1) == Some(&b'.') => dotted(1),
        b'.' => dotted(0),
        &c @ (b'-' | b'=') => {
            let n = run(c, 0);
            let style = if c == b'-' { EdgeStyle::Solid } else { EdgeStyle::Thick };
            if n >= 2
                && let Some(end) = edge_end_marker(b.get(n))
            {
                return Some((style, end, n + 1));
            }
            (n >= 3).then_some((style, EdgeEnd::None, n))
        }
        _ => None,
    }
}

/// A link at the start of `s`: `-->`, `---`, `-.->`, `==>`, `~~~`, `<-->`, `o--o`, `x--x`,
/// `--o`, `--x`, longer forms, `-- text -->` style labels and `-->|text|` labels.
/// `word_start`: `s` begins a word, so a leading `o` / `x` may be a start marker.
pub(super) fn match_flowchart_edge(s: &str, word_start: bool) -> Option<(EdgeTok, usize)> {
    let b = s.as_bytes();
    let marker_then_link = || matches!(b.get(1), Some(b'-' | b'=')) && matches!(b.get(2), Some(b'-' | b'=' | b'.'));
    let (start, i) = match b.first()? {
        b'<' if matches!(b.get(1), Some(b'-' | b'=')) => (EdgeEnd::Arrow, 1),
        b'o' if word_start && marker_then_link() => (EdgeEnd::Circle, 1),
        b'x' if word_start && marker_then_link() => (EdgeEnd::Cross, 1),
        b'-' | b'=' | b'~' => (EdgeEnd::None, 0),
        _ => return None,
    };
    let body = &s[i..];

    // Text labels inside the link: `-- text -->`, `-. text .->`, `== text ==>`.
    for (open, closer, style) in [("--", "--", EdgeStyle::Solid), ("-.", ".-", EdgeStyle::Dashed), ("==", "==", EdgeStyle::Thick)] {
        let Some(after) = body.strip_prefix(open) else { continue };
        if !after.starts_with(char::is_whitespace) {
            continue;
        }
        let mut search = 0usize;
        while let Some(rel) = after[search..].find(closer) {
            let at = search + rel;
            if let Some((close_style, end, len)) = match_edge_body(&after[at..])
                && close_style == style
            {
                let text = after[..at].trim();
                return Some((
                    EdgeTok {
                        style,
                        start,
                        end,
                        label: (!text.is_empty()).then(|| unquote(text)),
                        id: None,
                    },
                    i + open.len() + at + len,
                ));
            }
            search = at + closer.len();
        }
    }

    let (style, end, len) = match_edge_body(body)?;
    if style == EdgeStyle::Invisible && start != EdgeEnd::None {
        return None;
    }
    let mut consumed = i + len;
    let after = &s[consumed..];
    let after_ws = after.trim_start();
    let mut label = None;
    if let Some(after_pipe) = after_ws.strip_prefix('|')
        && let Some(close_pipe) = after_pipe.find('|')
    {
        let text = after_pipe[..close_pipe].trim();
        label = (!text.is_empty()).then(|| unquote(text));
        consumed += (after.len() - after_ws.len()) + 1 + close_pipe + 1;
    }
    Some((EdgeTok { style, start, end, label, id: None }, consumed))
}

pub(super) fn ensure_node_group(
    expr: &str,
    subgraph: Option<usize>,
    nodes: &mut Vec<FlowNode>,
    node_map: &mut HashMap<String, usize>,
) -> Vec<usize> {
    let mut ids = Vec::new();
    for part in expr.split('&') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let (p, class) = split_node_class(p);
        let (id, label_opt, shape_opt) = parse_node_token(p);
        if id.is_empty() {
            continue;
        }
        if let Some(&idx) = node_map.get(id) {
            if let Some(class) = class {
                nodes[idx].classes.push(class.to_string());
            }
            if let Some(lbl) = label_opt {
                nodes[idx].label = lbl;
            }
            if let Some(sh) = shape_opt {
                nodes[idx].shape = sh;
            }
            if let Some(media) = parse_node_media(p) {
                nodes[idx].media = Some(media);
            }
            if nodes[idx].subgraph.is_none() {
                nodes[idx].subgraph = subgraph;
            }
            ids.push(idx);
        } else {
            let idx = nodes.len();
            nodes.push(FlowNode {
                _id: id.to_string(),
                label: label_opt.unwrap_or_else(|| id.to_string()),
                shape: shape_opt.unwrap_or(NodeShape::Rect),
                subgraph,
                classes: class.map(|c| vec![c.to_string()]).unwrap_or_default(),
                media: parse_node_media(p),
                collapsed: false,
            });
            node_map.insert(id.to_string(), idx);
            ids.push(idx);
        }
    }
    ids
}

/// `A[Label]:::name` → (`A[Label]`, `name`). A `:::` inside the label is left alone.
pub(super) fn split_node_class(token: &str) -> (&str, Option<&str>) {
    if let Some(at) = token.rfind(":::") {
        let class = token[at + 3..].trim();
        if !class.is_empty() && !class.contains([']', ')', '}', '"']) {
            return (token[..at].trim(), Some(class));
        }
    }
    (token, None)
}

pub(super) fn parse_node_token(token: &str) -> (&str, Option<String>, Option<NodeShape>) {
    let t = token.trim();
    // `A@{ shape: cyl, label: "Database" }`. Unknown shapes keep the node's current shape.
    if let Some(at) = t.find("@{")
        && t.ends_with('}')
        && !t[..at].contains(['[', '(', '{', '>', '"', ' '])
    {
        let attrs = parse_brace_attrs(&t[at + 2..t.len() - 1]);
        let label = attrs.iter().find(|(k, _)| k == "label").map(|(_, v)| v.clone());
        let shape = attrs.iter().find(|(k, _)| k == "shape").and_then(|(_, v)| flow_shape_by_name(v));
        return (t[..at].trim(), label, shape);
    }
    // The shape starts at the first delimiter. A `>` inside `<br/>` is not asymmetric.
    let Some(open_at) = first_shape_delim(t) else {
        return (t, None, None);
    };
    let tail = &t[open_at..];
    for (open, close, shape) in [
        ("[(", ")]", NodeShape::Cylinder),
        ("([", "])", NodeShape::Stadium),
        ("[[", "]]", NodeShape::Subroutine),
        ("((", "))", NodeShape::Circle),
        ("{{", "}}", NodeShape::Hexagon),
        ("[/", "/]", NodeShape::Trapezoid),
        ("[/", "\\]", NodeShape::Parallelogram),
        ("[\\", "\\]", NodeShape::InvTrapezoid),
        ("[\\", "/]", NodeShape::Parallelogram),
        (">", "]", NodeShape::Asymmetric),
        ("[", "]", NodeShape::Rect),
        ("(", ")", NodeShape::Round),
        ("{", "}", NodeShape::Diamond),
    ] {
        if tail.starts_with(open) && t.ends_with(close) && t.len() >= open_at + open.len() + close.len() {
            let id = t[..open_at].trim();
            let inner = &t[open_at + open.len()..t.len() - close.len()];
            return (id, Some(unquote(inner.trim())), Some(shape));
        }
    }
    (t, None, None)
}

pub(super) fn first_shape_delim(token: &str) -> Option<usize> {
    let bytes = token.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'<' {
            if let Some(rel) = token[index + 1..].find('>') {
                index += 1 + rel + 1;
                continue;
            }
        }
        if matches!(bytes[index], b'[' | b'(' | b'{' | b'>') {
            if bytes[index] == b'>' && index > 0 && bytes[index - 1] == b'-' {
                index += 1;
                continue;
            }
            return Some(index);
        }
        index += token[index..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

pub(super) fn parse_flow_dir(token: &str) -> FlowDir {
    match token.trim().to_ascii_uppercase().as_str() {
        "LR" => FlowDir::Lr,
        "RL" => FlowDir::Rl,
        "BT" => FlowDir::Bt,
        _ => FlowDir::Td,
    }
}

/// Split a flowchart line on `;` that is outside brackets and quotes.
pub(super) fn split_flow_statements(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (idx, ch) in line.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => {
                let stmt = line[start..idx].trim();
                if !stmt.is_empty() {
                    out.push(stmt);
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let rest = line[start..].trim().trim_end_matches(';').trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// The white box behind an edge label: `(width, height)`.
pub(super) fn flow_edge_label_size(label: &str) -> (f32, f32) {
    let widths = label_line_widths(label, 140.0, 11.0);
    (widths.iter().copied().fold(0.0f32, f32::max) + 10.0, widths.len() as f32 * 14.0 + 6.0)
}

pub(super) fn flow_node_size(shape: NodeShape, label: &str, horizontal: bool) -> (f32, f32) {
    let max_text = 168.0f32;
    let widths = label_line_widths(label, max_text, 13.0);
    let tw = widths.iter().copied().fold(0.0f32, f32::max);
    let text_h = widths.len() as f32 * 16.0;
    match shape {
        NodeShape::Ext(ext) => flow_ext_size(ext, tw, text_h, horizontal),
        NodeShape::Diamond | NodeShape::Hexagon => ((tw + 56.0).max(96.0), (text_h + 28.0).max(54.0)),
        NodeShape::Circle => {
            let d = (tw + 28.0).max(text_h + 24.0).max(54.0);
            (d, d)
        }
        NodeShape::Asymmetric => ((tw + 48.0).max(96.0), (text_h + 20.0).max(40.0)),
        _ => ((tw + 36.0).max(84.0), (text_h + 18.0).max(40.0)),
    }
}

/// Node centers, canvas size and each subgraph's frame. Every subgraph is laid out as one
/// block (nested ones inside their parent), so frames of different subgraphs never overlap.
pub(super) fn layout_flowchart(
    member_of: &[Option<usize>],
    edge_pairs: &[(usize, usize)],
    node_sizes: &[(f32, f32)],
    subgraphs: &[SubgraphInfo],
    dir: FlowDir,
    layer_gap: f32,
    node_gap: f32,
) -> GroupedLayout {
    let flags = |d: FlowDir| (matches!(d, FlowDir::Lr | FlowDir::Rl), matches!(d, FlowDir::Bt | FlowDir::Rl));
    let (horizontal, reversed) = flags(dir);
    let margin = if subgraphs.is_empty() { 32.0f32 } else { 44.0f32 };
    if subgraphs.is_empty() {
        let (centers, w, h) = compute_layered_layout(node_sizes.len(), edge_pairs, node_sizes, horizontal, reversed, margin, layer_gap, node_gap);
        return (centers, w, h, Vec::new());
    }
    let groups: Vec<NodeGroup> = subgraphs.iter().map(|sg| NodeGroup { title: sg.title.clone(), parent: sg.parent }).collect();
    let dirs: Vec<Option<(bool, bool)>> = subgraphs.iter().map(|sg| sg.dir.map(flags)).collect();
    layout_with_group_dirs(node_sizes, edge_pairs, member_of, &groups, &dirs, horizontal, reversed, margin, layer_gap, node_gap)
}

/// Which nodes stand for a subgraph and which are hidden in a closed one.
pub(super) struct SubgraphEnds {
    /// A node whose id is a subgraph's (`A --> TOP`): drawn as that subgraph's frame.
    pub(super) alias: Vec<Option<usize>>,
    /// Inside a `view: collapsed` subgraph: not drawn; its edges go to the closed box.
    pub(super) hidden: Vec<bool>,
}

/// Resolves subgraph ids used as edge ends, and closes `view: collapsed` subgraphs: their
/// node becomes a box titled like the subgraph, and edges to anything inside it go to the box.
pub(super) fn resolve_subgraph_ends(
    nodes: &mut Vec<FlowNode>,
    node_map: &mut HashMap<String, usize>,
    edges: &mut [FlowEdge],
    subgraphs: &[SubgraphInfo],
    collapsed_ids: &[String],
) -> SubgraphEnds {
    let within = |mut g: Option<usize>, target: usize| {
        while let Some(cur) = g {
            if cur == target {
                return true;
            }
            g = subgraphs[cur].parent;
        }
        false
    };
    let mut hidden = vec![false; nodes.len()];
    // Outermost closed subgraphs first: (subgraph, its box).
    let mut closed: Vec<(usize, usize)> = Vec::new();
    for (sg, info) in subgraphs.iter().enumerate() {
        if !collapsed_ids.contains(&info.id) || closed.iter().any(|&(outer, _)| within(Some(sg), outer)) {
            continue;
        }
        let idx = match node_map.get(&info.id) {
            Some(&idx) => idx,
            None => {
                nodes.push(FlowNode {
                    _id: info.id.clone(),
                    label: String::new(),
                    shape: NodeShape::Round,
                    subgraph: info.parent,
                    classes: Vec::new(),
                    media: None,
                    collapsed: false,
                });
                hidden.push(false);
                node_map.insert(info.id.clone(), nodes.len() - 1);
                nodes.len() - 1
            }
        };
        if within(nodes[idx].subgraph, sg) {
            nodes[idx].subgraph = info.parent;
        }
        nodes[idx].label = info.title.clone();
        nodes[idx].collapsed = true;
        closed.push((sg, idx));
    }
    let mut redirect: Vec<usize> = (0..nodes.len()).collect();
    for &(sg, boxed) in &closed {
        for j in 0..nodes.len() {
            if j != boxed && within(nodes[j].subgraph, sg) {
                hidden[j] = true;
                redirect[j] = boxed;
            }
        }
    }
    for e in edges.iter_mut() {
        e.from = redirect[e.from];
        e.to = redirect[e.to];
        if e.from == e.to && (hidden[e.from] || nodes[e.from].collapsed) {
            // Both ends were inside the closed subgraph.
            e.style = EdgeStyle::Invisible;
        }
    }
    let mut alias = vec![None; nodes.len()];
    for (sg, info) in subgraphs.iter().enumerate() {
        if closed.iter().any(|&(c, _)| c == sg) {
            continue;
        }
        if let Some(&i) = node_map.get(&info.id)
            && !hidden[i]
            && !nodes[i].collapsed
            && !within(nodes[i].subgraph, sg)
            && (0..nodes.len()).any(|j| j != i && !hidden[j] && within(nodes[j].subgraph, sg))
        {
            alias[i] = Some(sg);
        }
    }
    // An alias that sits inside another alias's subgraph is still a frame, not a member.
    SubgraphEnds { alias, hidden }
}

/// An `@{ icon }` / `@{ img }` node: the picture plus a label line above or below it.
pub(super) fn flow_media_size(media: &FlowMedia, label: &str) -> (f32, f32) {
    let label_w = label_line_widths(label, 168.0, 12.0).into_iter().fold(0.0f32, f32::max);
    let lines = if label.is_empty() { 0 } else { label_lines(label, 168.0, 12.0).len() };
    ((media.w.max(label_w + 8.0)).max(24.0), media.h + lines as f32 * 15.0 + 6.0)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_flow_media(svg: &mut String, media: &FlowMedia, label: &str, x: f32, y: f32, w: f32, h: f32, style: &FlowStyle) {
    let text_h = h - media.h - 6.0;
    let (py, ly) = if media.label_on_top { (y + text_h + 6.0, y) } else { (y, y + media.h + 6.0) };
    let px = x + (w - media.w) / 2.0;
    let (pw, ph) = (media.w, media.h);
    let shape_at = svg.len();
    match &media.kind {
        FlowMediaKind::Icon(name) => {
            let rx = match media.form.as_str() {
                "circle" => pw.min(ph) / 2.0,
                "rounded" => 8.0,
                "square" => 2.0,
                _ => 4.0,
            };
            svg.push_str(&format!(
                r##"<rect x="{px:.1}" y="{py:.1}" width="{pw:.1}" height="{ph:.1}" rx="{rx:.1}" fill="#eff6ff" stroke="#2563eb" stroke-width="1.4" data-icon="{}"/>"##,
                escape_attr(name)
            ));
            let glyph = name.split_once(':').map_or("\u{25C6}", |(_, n)| fa_symbol(n));
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="{:.1}" fill="#1e3a8a">{glyph}</text>"##,
                px + pw / 2.0,
                py + ph / 2.0 + ph * 0.18,
                ph * 0.5
            ));
        }
        FlowMediaKind::Image(url) => {
            // No fetching: a frame with a picture symbol keeps the image's place.
            svg.push_str(&format!(
                r##"<rect x="{px:.1}" y="{py:.1}" width="{pw:.1}" height="{ph:.1}" fill="#f1f5f9" stroke="#94a3b8" stroke-width="1" data-img="{}"/>"##,
                escape_attr(url)
            ));
            let (ix, iy, iw, ih) = (px + pw * 0.2, py + ph * 0.2, pw * 0.6, ph * 0.6);
            svg.push_str(&format!(
                r##"<path d="M {ix:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z" fill="#cbd5e1"/><circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="#cbd5e1"/>"##,
                iy + ih,
                ix + iw * 0.35,
                iy + ih * 0.35,
                ix + iw * 0.6,
                iy + ih * 0.7,
                ix + iw * 0.75,
                iy + ih * 0.5,
                ix + iw,
                iy + ih,
                ix + iw * 0.78,
                iy + ih * 0.18,
                (iw * 0.1).max(2.0)
            ));
        }
    }
    if !style.is_empty() {
        let shape_svg = svg.split_off(shape_at);
        svg.push_str(&restyle_svg_fragment(&shape_svg, style));
    }
    if !label.is_empty() {
        let fill = style.color.as_deref().map(escape_attr).unwrap_or_else(|| "#0f172a".to_string());
        push_centered_label(svg, x + w / 2.0, ly + text_h / 2.0, label, 12.0, 168.0, &fill, "500");
    }
}

/// Draw a flowchart node shape in the box `x, y, w, h` (centre `cx, cy`). Returns how far the label
/// moves down from the centre (some expanded shapes keep their text off-centre).
#[allow(clippy::too_many_arguments)]
pub(super) fn push_flow_shape(svg: &mut String, shape: NodeShape, x: f32, y: f32, w: f32, h: f32, cx: f32, cy: f32) -> f32 {
    match shape {
        NodeShape::Rect => {
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3" fill="#eff6ff" stroke="#2563eb" stroke-width="1.4" data-shape="rect"/>"##
            ));
        }
        NodeShape::Round => {
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="10" fill="#eff6ff" stroke="#2563eb" stroke-width="1.4" data-shape="round"/>"##
            ));
        }
        NodeShape::Stadium => {
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{:.1}" fill="#f0fdf4" stroke="#16a34a" stroke-width="1.4" data-shape="stadium"/>"##,
                h / 2.0
            ));
        }
        NodeShape::Subroutine => {
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3" fill="#eff6ff" stroke="#2563eb" stroke-width="1.4" data-shape="subroutine"/>"##
            ));
            svg.push_str(&format!(
                r##"<line x1="{:.1}" y1="{y:.1}" x2="{:.1}" y2="{:.1}" stroke="#2563eb" stroke-width="1.2"/>"##,
                x + 7.0, x + 7.0, y + h
            ));
            svg.push_str(&format!(
                r##"<line x1="{:.1}" y1="{y:.1}" x2="{:.1}" y2="{:.1}" stroke="#2563eb" stroke-width="1.2"/>"##,
                x + w - 7.0, x + w - 7.0, y + h
            ));
        }
        NodeShape::Diamond => {
            svg.push_str(&format!(
                r##"<polygon points="{cx:.1},{y:.1} {:.1},{cy:.1} {cx:.1},{:.1} {x:.1},{cy:.1}" fill="#fefce8" stroke="#ca8a04" stroke-width="1.4" data-shape="diamond"/>"##,
                x + w, y + h
            ));
        }
        NodeShape::Circle => {
            svg.push_str(&format!(
                r##"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{:.1}" ry="{:.1}" fill="#f5f3ff" stroke="#7c3aed" stroke-width="1.4" data-shape="circle"/>"##,
                w / 2.0, h / 2.0
            ));
        }
        NodeShape::Cylinder => push_flow_cylinder(svg, x, y, w, h),
        NodeShape::Hexagon => push_flow_polygon(svg, &hexagon_points(x, y, w, h), "#f5f3ff", "#7c3aed", "hexagon"),
        NodeShape::Trapezoid => push_flow_polygon(svg, &trapezoid_points(x, y, w, h, true), "#eff6ff", "#2563eb", "trapezoid"),
        NodeShape::InvTrapezoid => push_flow_polygon(svg, &trapezoid_points(x, y, w, h, false), "#eff6ff", "#2563eb", "inv-trapezoid"),
        NodeShape::Parallelogram => push_flow_polygon(svg, &parallelogram_points(x, y, w, h), "#eff6ff", "#2563eb", "parallelogram"),
        NodeShape::Asymmetric => push_flow_polygon(svg, &asymmetric_points(x, y, w, h), "#eff6ff", "#2563eb", "asymmetric"),
        NodeShape::Ext(ext) => {
            svg.push_str(r##"<g data-shape="ext">"##);
            let dy = push_ext_shape(svg, ext, x, y, w, h);
            svg.push_str("</g>");
            return dy;
        }
    }
    0.0
}

pub(super) fn push_flow_polygon(svg: &mut String, points: &[(f32, f32)], fill: &str, stroke: &str, shape: &str) {
    let body: String = points.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect::<Vec<_>>().join(" ");
    svg.push_str(&format!(
        r##"<polygon points="{body}" fill="{fill}" stroke="{stroke}" stroke-width="1.4" data-shape="{shape}"/>"##
    ));
}

pub(super) fn push_flow_cylinder(svg: &mut String, x: f32, y: f32, w: f32, h: f32) {
    let ry = (h * 0.18).clamp(6.0, 14.0);
    let body_y = y + ry;
    let body_h = (h - ry * 2.0).max(8.0);
    svg.push_str(&format!(
        r##"<rect x="{x:.1}" y="{body_y:.1}" width="{w:.1}" height="{body_h:.1}" fill="#ecfeff" stroke="#0891b2" stroke-width="1.4" data-shape="cylinder"/>"##
    ));
    svg.push_str(&format!(
        r##"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{ry:.1}" fill="#ecfeff" stroke="#0891b2" stroke-width="1.4"/>"##,
        x + w / 2.0,
        y + h - ry,
        w / 2.0
    ));
    svg.push_str(&format!(
        r##"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{ry:.1}" fill="#cffafe" stroke="#0891b2" stroke-width="1.4"/>"##,
        x + w / 2.0,
        y + ry,
        w / 2.0
    ));
}

pub(super) fn hexagon_points(x: f32, y: f32, w: f32, h: f32) -> Vec<(f32, f32)> {
    let inset = w * 0.18;
    vec![(x + inset, y), (x + w - inset, y), (x + w, y + h / 2.0), (x + w - inset, y + h), (x + inset, y + h), (x, y + h / 2.0)]
}

pub(super) fn trapezoid_points(x: f32, y: f32, w: f32, h: f32, narrow_top: bool) -> Vec<(f32, f32)> {
    let inset = w * 0.16;
    if narrow_top {
        vec![(x + inset, y), (x + w - inset, y), (x + w, y + h), (x, y + h)]
    } else {
        vec![(x, y), (x + w, y), (x + w - inset, y + h), (x + inset, y + h)]
    }
}

pub(super) fn parallelogram_points(x: f32, y: f32, w: f32, h: f32) -> Vec<(f32, f32)> {
    let skew = w * 0.16;
    vec![(x + skew, y), (x + w, y), (x + w - skew, y + h), (x, y + h)]
}

pub(super) fn asymmetric_points(x: f32, y: f32, w: f32, h: f32) -> Vec<(f32, f32)> {
    let inset = w * 0.16;
    vec![(x, y), (x + w - inset, y), (x + w, y + h / 2.0), (x + w - inset, y + h), (x, y + h)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn renders_flowchart_svg_with_shapes_subgraphs_and_cycles() {
        let flow = "\
flowchart LR
  subgraph Input
    A([開始]) --> B[データ検証]
  end
  B -->|OK| C{分岐}
  C -->|Yes| D((完了))
  C -->|Retry| B
";
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(flow)
        else {
            panic!("expected svg for flowchart");
        };
        assert!(width_px > 250 && height_px > 100);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains("開始"));
        assert!(
            svg.contains(r#"data-shape="stadium""#)
                && svg.contains(r#"data-shape="rect""#)
                && svg.contains(r#"data-shape="diamond""#),
            "{svg}"
        );
        assert!(svg.contains("データ検証"));
        assert!(svg.contains("分岐"));
        assert!(svg.contains("完了"));
        assert!(svg.contains("Input"));
        assert!(svg.contains("OK"));
    }

    #[test]
    fn flowchart_labels_shapes_statements_and_subgraph_direction() {
        let labeled = "\
flowchart TD
  A[\"上<br/>下\"]
  B[日本語の長い見出しを折り返して枠を広げる確認]
";
        let svg = svg_of(labeled);
        assert!(svg.contains(">上<") && svg.contains(">下<"));
        assert!(!svg.contains("<br") && !svg.contains("&lt;br"));
        let long = "日本語の長い見出しを折り返して枠を広げる確認";
        let lines = label_lines(long, 168.0, 13.0);
        assert!(lines.len() >= 2, "{lines:?}");
        for line in &lines {
            assert!(svg.contains(line), "missing wrapped line {line}");
        }
        assert!(!svg.contains(long));

        let shapes = "\
flowchart LR
  A[(筒)] --> B{{六角}}
  C[/台形/] --> D[\\逆台形\\]
  E[/平行四辺形\\] --> F>旗]
";
        let svg = svg_of(shapes);
        for shape in ["cylinder", "hexagon", "trapezoid", "inv-trapezoid", "parallelogram", "asymmetric"] {
            assert!(svg.contains(&format!("data-shape=\"{shape}\"")), "missing {shape}:\n{svg}");
        }
        for label in ["筒", "六角", "台形", "逆台形", "平行四辺形", "旗"] {
            assert!(svg.contains(label), "missing {label}");
        }

        let split = "flowchart LR\n  A[A]-->B[B]; C[C]-->D[D]\n";
        let svg = svg_of(split);
        for label in ["A", "B", "C", "D"] {
            assert!(svg.contains(&format!(">{label}<")));
        }
        assert!(svg.matches("marker-end=\"url(#flow-arrow)\"").count() >= 2);

        let directed = "\
flowchart TD
  subgraph side
    direction LR
    L[左] --> R[右]
  end
";
        let svg = svg_of(directed);
        assert!(text_x(&svg, "左") < text_x(&svg, "右"), "subgraph LR should place 左 left of 右:\n{svg}");
    }

    #[test]
    fn flowchart_siblings_keep_wrapped_label_gaps() {
        let long = "日本語の長い見出しを折り返して枠を広げる確認";
        let src = format!("flowchart TD\n  T[親] --> A[短い]\n  T --> B[{long}]\n");
        let svg = svg_of(&src);
        let (vw, vh) = view_box(&svg);
        let rects = painted_rects(&svg, "#eff6ff");
        assert!(rects.len() >= 3, "{svg}");
        for rect in &rects {
            assert!(rect.0 >= -0.5 && rect.1 >= -0.5 && rect.0 + rect.2 <= vw + 0.5 && rect.1 + rect.3 <= vh + 0.5, "node outside viewBox {rect:?} {vw}x{vh}");
        }
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(!rects_overlap(rects[i], rects[j]), "nodes overlap: {:?} {:?}", rects[i], rects[j]);
            }
        }
        let short = rects.iter().copied().find(|rect| rect_holds(*rect, text_x(&svg, "短い"), text_y(&svg, "短い"))).expect("short node");
        let first = label_lines(long, 168.0, 13.0).into_iter().next().unwrap();
        let wide = rects.iter().copied().find(|rect| rect_holds(*rect, text_x(&svg, &first), text_y(&svg, &first))).expect("wide node");
        assert!(wide.2 > short.2 + 50.0, "wrapped label should widen its box: {wide:?} vs {short:?}");
        let gap = if short.0 + short.2 <= wide.0 {
            wide.0 - (short.0 + short.2)
        } else {
            short.0 - (wide.0 + wide.2)
        };
        assert!((gap - 34.0).abs() < 0.6, "sibling gap {gap}");
        assert!(svg.contains(&first));
    }

    #[test]
    fn edges_detour_around_intermediate_nodes() {
        let sizes = [(84.0f32, 40.0), (114.0, 40.0), (84.0, 40.0)];
        let pairs = [(0usize, 1usize), (0, 2), (1, 2)];
        let (centers, total_w, total_h) = compute_layered_layout(3, &pairs, &sizes, false, false, 32.0, 64.0, 34.0);
        let canvas = (total_w as f32, total_h as f32);
        let skip = route_layered_edge(0, 2, &centers, &sizes, false, true, canvas);
        let EdgeRouteKind::Path { d, detour: true } = &skip.kind else {
            panic!("skip edge should detour: {skip:?}");
        };
        let mid = node_rect(centers[1].0, centers[1].1, sizes[1].0, sizes[1].1);
        for (px, py) in sample_path(d) {
            assert!(!point_in_rect(px, py, &pad_rect(mid, -1.0)), "detour sample {px},{py} enters the middle node");
            assert!(px >= -0.5 && py >= -0.5 && px <= canvas.0 + 0.5 && py <= canvas.1 + 0.5);
        }
        assert!(!point_in_rect(skip.label_at.0, skip.label_at.1, &mid), "label sits inside the obstacle");
        let adjacent = route_layered_edge(0, 1, &centers, &sizes, false, true, canvas);
        assert!(matches!(adjacent.kind, EdgeRouteKind::Path { detour: false, .. }), "a clear edge keeps its curve: {adjacent:?}");

        let blocked = route_layered_edge(
            0,
            2,
            &[(40.0, 12.0), (40.0, 50.0), (40.0, 88.0)],
            &[(24.0, 16.0), (200.0, 200.0), (24.0, 16.0)],
            false,
            true,
            (80.0, 100.0),
        );
        let EdgeRouteKind::Path { d: blocked_d, detour: false } = &blocked.kind else {
            panic!("an unavoidable crossing keeps the curve: {blocked:?}");
        };
        assert!(blocked_d.contains('C'), "{blocked_d}");

        let svg = svg_of("\
flowchart TD
  Top[上] --> Mid[中間障害物]
  Top -->|経由| Bottom[下]
  Mid --> Bottom
");
        let (vw, vh) = view_box(&svg);
        for label in ["上", "中間障害物", "下", "経由"] {
            assert!(svg.contains(&format!(">{label}<")), "{label} missing:\n{svg}");
        }
        let rects = painted_rects(&svg, "#eff6ff");
        assert_eq!(rects.len(), 3, "{svg}");
        for rect in &rects {
            assert!(rect.0 >= -0.5 && rect.1 >= -0.5 && rect.0 + rect.2 <= vw + 0.5 && rect.1 + rect.3 <= vh + 0.5, "{rect:?} outside {vw}x{vh}");
        }
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(!rects_overlap(rects[i], rects[j]));
            }
        }
        let mid_rect = rects.iter().copied().find(|rect| rect_holds(*rect, text_x(&svg, "中間障害物"), text_y(&svg, "中間障害物"))).expect("middle");
        let paths = fill_none_paths(&svg);
        let detours: Vec<_> = paths.iter().filter(|(_, detour)| *detour).collect();
        assert_eq!(detours.len(), 1, "only the skip edge detours:\n{svg}");
        let samples = sample_path(&detours[0].0);
        assert!(samples.iter().all(|(px, py)| !inside_inset(*px, *py, mid_rect)), "path enters 中間障害物:\n{svg}");
        let (lx, ly) = (text_x(&svg, "経由"), text_y(&svg, "経由"));
        assert!(!rect_holds(mid_rect, lx, ly), "edge label is inside the middle node");
        assert!(samples.iter().any(|(px, py)| (px - lx).hypot(py - ly) < 24.0), "label should sit on the detour");
        assert!(paths.iter().filter(|(_, detour)| !detour).all(|(d, _)| d.contains('C')));

        let plain = svg_of("flowchart TD\n  A[上] --> B[下]\n");
        let plain_paths = fill_none_paths(&plain);
        assert_eq!(plain_paths.len(), 1);
        assert!(!plain_paths[0].1 && plain_paths[0].0.contains('C'));

        let state = svg_of("\
stateDiagram-v2
  A --> B
  A --> C
  B --> C
");
        assert!(state.contains("data-route=\"detour\""), "{state}");
        for label in ["A", "B", "C"] {
            assert!(state.contains(&format!(">{label}<")));
        }
        let class = svg_of("\
classDiagram
  class A
  class B
  class C
  A --> C
  A --> B
  B --> C
");
        assert!(class.contains("data-route=\"detour\""), "{class}");
        assert!(class.contains(">A<") && class.contains(">B<") && class.contains(">C<"));
        let er = svg_of("\
erDiagram
  A ||--|| C : skip
  A ||--|| B : near
  B ||--|| C : next
");
        assert!(er.contains("data-route=\"detour\""), "{er}");
        assert!(er.contains("skip") && er.contains(">A<") && er.contains(">B<") && er.contains(">C<"));
    }

    #[test]
    fn subgraph_frame_pads_title_away_from_children() {
        let svg = svg_of("\
flowchart TD
  subgraph 外側
    A[子ノード]
  end
");
        let (vw, vh) = view_box(&svg);
        let frames = painted_rects(&svg, "#f8fafc");
        let nodes = painted_rects(&svg, "#eff6ff");
        assert_eq!(frames.len(), 1, "{svg}");
        assert_eq!(nodes.len(), 1, "{svg}");
        let frame = frames[0];
        let node = nodes[0];
        assert!((node.0 - frame.0 - 16.0).abs() < 0.2, "left pad {frame:?} {node:?}");
        assert!(((frame.0 + frame.2) - (node.0 + node.2) - 16.0).abs() < 0.2, "right pad");
        assert!(((frame.1 + frame.3) - (node.1 + node.3) - 16.0).abs() < 0.2, "bottom pad");
        assert!((node.1 - frame.1 - GROUP_TOP).abs() < 0.2, "title band {frame:?} {node:?}");
        let title_y = text_y(&svg, "外側");
        assert!((title_y - (frame.1 + 16.0)).abs() < 0.2, "title baseline {title_y}");
        assert!(node.1 - title_y >= 16.0, "title overlaps the child");
        assert!(frame.0 >= -0.5 && frame.1 >= -0.5 && frame.0 + frame.2 <= vw + 0.5 && frame.1 + frame.3 <= vh + 0.5, "frame outside viewBox");
        assert!(node.0 >= -0.5 && node.1 >= -0.5 && node.0 + node.2 <= vw + 0.5 && node.1 + node.3 <= vh + 0.5);
        assert!(svg.contains(">子ノード<"));
    }

    // ---- P0 compatibility additions --------------------------------------------------------

    #[test]
    fn flowchart_bidirectional_circle_cross_and_invisible_edges() {
        let tok = |s: &str| match_flowchart_edge(s, true).map(|(t, n)| (t.style, t.start, t.end, t.label, n));
        assert_eq!(tok("<--> B"), Some((EdgeStyle::Solid, EdgeEnd::Arrow, EdgeEnd::Arrow, None, 4)));
        assert_eq!(tok("o--o B"), Some((EdgeStyle::Solid, EdgeEnd::Circle, EdgeEnd::Circle, None, 4)));
        assert_eq!(tok("x--x B"), Some((EdgeStyle::Solid, EdgeEnd::Cross, EdgeEnd::Cross, None, 4)));
        assert_eq!(tok("--o B"), Some((EdgeStyle::Solid, EdgeEnd::None, EdgeEnd::Circle, None, 3)));
        assert_eq!(tok("--x B"), Some((EdgeStyle::Solid, EdgeEnd::None, EdgeEnd::Cross, None, 3)));
        assert_eq!(tok("<-.-> B"), Some((EdgeStyle::Dashed, EdgeEnd::Arrow, EdgeEnd::Arrow, None, 5)));
        assert_eq!(tok("<==> B"), Some((EdgeStyle::Thick, EdgeEnd::Arrow, EdgeEnd::Arrow, None, 4)));
        assert_eq!(tok("----> B"), Some((EdgeStyle::Solid, EdgeEnd::None, EdgeEnd::Arrow, None, 5)));
        assert_eq!(tok("~~~ B"), Some((EdgeStyle::Invisible, EdgeEnd::None, EdgeEnd::None, None, 3)));
        assert_eq!(tok("<-- text --> B"), Some((EdgeStyle::Solid, EdgeEnd::Arrow, EdgeEnd::Arrow, Some("text".into()), 12)));
        assert_eq!(tok("-. 点線 .-> B").map(|t| (t.0, t.2, t.3)), Some((EdgeStyle::Dashed, EdgeEnd::Arrow, Some("点線".into()))));
        assert_eq!(tok("-->|yes| B").map(|t| t.3), Some(Some("yes".into())));
        assert_eq!(tok("-- B"), None);
        // `o` / `x` start a link only at the start of a word: `Bo--x` is not a circle start.
        assert!(match_flowchart_edge("o--o B", false).is_none());

        let svg = svg_of("flowchart LR\n  A <--> B\n  B o--o C\n  C x--x D\n  D --o E\n  E ~~~ F\n");
        for label in ["A", "B", "C", "D", "E", "F"] {
            assert!(svg.contains(&format!(">{label}<")), "{label} missing:\n{svg}");
        }
        assert!(!svg.contains(">A <<") && !svg.contains("&lt;"), "no stray node text: {svg}");
        assert_eq!(svg.matches(r##"marker-start="url(#flow-arrow)""##).count(), 1);
        assert_eq!(svg.matches(r##"marker-start="url(#flow-circle)""##).count(), 1);
        assert_eq!(svg.matches(r##"marker-start="url(#flow-cross)""##).count(), 1);
        assert_eq!(svg.matches(r##"marker-end="url(#flow-circle)""##).count(), 2);
        assert_eq!(svg.matches(r##"marker-end="url(#flow-cross)""##).count(), 1);
        // The `~~~` link places F after E but draws nothing.
        let body = &svg[svg.find("</defs>").expect("defs")..];
        assert_eq!(fill_none_paths(body).len() + body.matches("<line x1").count(), 4, "{svg}");
        assert!(text_x(&svg, "F") > text_x(&svg, "E"));
        // Edge ids are not node text.
        let ids = svg_of("flowchart LR\n  A e1@--> B\n");
        assert!(ids.contains(">A<") && !ids.contains("e1"), "{ids}");
    }

    #[test]
    fn flowchart_class_def_class_style_and_link_style() {
        let svg = svg_of(
            "flowchart LR
  A:::warn --> B
  B --> C
  C --> D
  classDef warn fill:#fee2e2,stroke:#dc2626,stroke-width:3px
  classDef ok fill:#dcfce7
  class B,C ok
  style D fill:#f9f,stroke:#333,color:#ffffff,stroke-dasharray: 5 5
  linkStyle 1 stroke:#ff3,stroke-width:4px
  linkStyle default stroke:#0000ff
",
        );
        assert!(svg.contains(r##"fill="#fee2e2" stroke="#dc2626" stroke-width="3""##), "{svg}");
        assert_eq!(painted_rects(&svg, "#dcfce7").len(), 2, "class B,C ok:\n{svg}");
        assert_eq!(painted_rects(&svg, "#f9f").len(), 1);
        assert!(svg.contains(r##"stroke-dasharray="5 5""##));
        assert!(svg.contains(r##"fill="#ffffff">D</text>"##), "style color is the label colour:\n{svg}");
        // linkStyle: the second edge is yellow and wide, the others take the default.
        assert_eq!(svg.matches(r##"stroke="#ff3" stroke-width="4""##).count(), 1, "{svg}");
        assert_eq!(svg.matches(r##"stroke="#0000ff""##).count(), 2);
        // `classDef default` applies to every node without being named.
        let def = svg_of("flowchart LR\n  A --> B\n  classDef default fill:#abcdef\n");
        assert_eq!(painted_rects(&def, "#abcdef").len(), 2);
        // Values that could break out of the attribute are dropped.
        let evil = svg_of("flowchart LR\n  A\n  style A fill:red\"/><script>,stroke:#123456\n");
        assert!(!evil.contains("<script>") && evil.contains(r##"stroke="#123456""##), "{evil}");
        assert!(roxmltree::Document::parse(&evil).is_ok());
        assert_eq!(split_node_class("A[x]:::hot"), ("A[x]", Some("hot")));
        assert_eq!(split_node_class("A[a:::b]"), ("A[a:::b]", None));
        assert_eq!(parse_flow_style("fill:rgb(1, 2, 3),stroke-width:2px").fill.as_deref(), Some("rgb(1, 2, 3)"));
    }

    #[test]
    fn flowchart_expanded_shapes_labels_and_edge_attributes() {
        let svg = svg_of(
            "flowchart TD\n  A@{ shape: cyl, label: \"Orders, 2026\" } --> B@{ shape: doc }\n  B --> C@{ shape: sm-circ, label: \"hidden\" }\n  C --> D@{ shape: no-such-shape, label: \"Kept\" }\n",
        );
        assert!(svg.contains(r#"data-shape="cylinder""#), "{svg}");
        assert!(svg.contains(r#"data-shape="document""#));
        assert!(svg.contains(r#"data-shape="small-circle""#));
        // The label comes from the attributes (comma inside quotes kept); no id or raw attribute text leaks.
        assert!(svg.contains(">Orders, 2026<"));
        assert!(svg.contains(">B<") && svg.contains(">Kept<"));
        assert!(!svg.contains("hidden") && !svg.contains("shape:") && !svg.contains("A@"));
        // An unknown shape name keeps the default rectangle.
        assert_eq!(svg.matches(r##"rx="3" fill="#eff6ff""##).count(), 1, "only Kept is a plain box\n{svg}");

        // Every name and alias in the table draws something different from a plain box or falls back to a known shape.
        for name in [
            "rect", "rounded", "stadium", "subproc", "cyl", "circle", "odd", "diamond", "hex", "lean-r", "lean-l", "trap-b",
            "trap-t", "dbl-circ", "text", "notch-rect", "lin-rect", "sm-circ", "framed-circle", "f-circ", "fork", "hourglass",
            "comment", "brace-r", "braces", "bolt", "doc", "lin-doc", "docs", "tag-doc", "delay", "das", "lin-cyl", "curv-trap",
            "div-rect", "tri", "flip-tri", "win-pane", "notch-pent", "sl-rect", "procs", "flag", "bow-rect", "cross-circ",
            "tag-rect", "datastore", "database", "decision", "junction", "manual-input", "stored-data",
        ] {
            assert!(flow_shape_by_name(name).is_some(), "{name}");
            let svg = svg_of(&format!("flowchart LR\n  X@{{ shape: {name}, label: \"L\" }} --> Y\n"));
            assert!(roxmltree::Document::parse(&svg).is_ok(), "{name}: {svg}");
        }
        // Start/stop markers and fork bars are small and carry no text, as in mermaid.
        let fork = svg_of("flowchart LR\n  A --> J@{ shape: fork, label: \"Join\" } --> B\n");
        let bar = tag_with(&fork, r#"data-shape="fork""#)[0];
        assert!(attr_f32(bar, "height").unwrap() > attr_f32(bar, "width").unwrap(), "vertical bar in LR: {bar}");
        assert!(!fork.contains(">Join<"));

        // `e1@{ ... }` styles the edge declared with that id instead of creating a node.
        let src = "flowchart LR\n  A e1@--> B\n  A e2@--> C\n  e1@{ curve: linear }\n  e2@{ animate: true }\n";
        let svg = svg_of(src);
        assert!(!svg.contains(">e1<") && !svg.contains(">e2<"), "{svg}");
        let mut nodes = Vec::new();
        let mut node_map = HashMap::new();
        let mut edges = Vec::new();
        parse_flowchart_statement("A e1@--> B", None, &mut nodes, &mut node_map, &mut edges);
        assert_eq!(edges[0].id.as_deref(), Some("e1"));
        assert_eq!(split_attr_statement("e1@{ curve: linear }"), Some(("e1", vec![("curve".to_string(), "linear".to_string())])));
        assert_eq!(split_attr_statement("A[x] --> B"), None);
        // A node id that happens to be reused after an edge id still parses as a node.
        let svg = svg_of("flowchart LR\n  n1@{ shape: hex, label: \"Six\" } --> n2\n");
        assert!(svg.contains(r#"data-shape="hexagon""#) && svg.contains(">Six<"));
    }

    #[test]
    fn flowchart_markdown_strings_and_entity_codes() {
        let src = "flowchart LR\n    markdown[\"`This **is** _Markdown_`\"]\n    newLines[\"`Line1\n    Line 2\n    Line 3`\"]\n    markdown --> newLines\n";
        let svg = svg_of(src);
        assert!(svg.contains(r#">This <tspan font-weight="bold">is</tspan> <tspan font-style="italic">Markdown</tspan></text>"#), "{svg}");
        for line in ["Line1", "Line 2", "Line 3"] {
            assert!(svg.contains(&format!(">{line}</text>")), "{line}: {svg}");
        }
        assert!(!svg.contains('`'));

        let sub = svg_of("flowchart LR\nsubgraph \"`**Two**`\"\n  c(\"x\") -- \"`Bold **edge label**`\" --> d(\"y\")\nend\n");
        assert!(sub.contains(r#"<tspan font-weight="bold">Two</tspan>"#), "{sub}");
        assert!(sub.contains(r#">Bold <tspan font-weight="bold">edge label</tspan></text>"#), "{sub}");

        let ent = svg_of("flowchart LR\n    A[\"A double quote:#quot;\"] --> B[\"A dec char:#9829;\"]\n");
        assert!(ent.contains(">A double quote:&quot;</text>") && ent.contains(">A dec char:♥</text>"), "{ent}");
        // A plain label with asterisks is not Markdown.
        assert!(svg_of("flowchart LR\n  a[\"2 ** 3\"]\n").contains(">2 ** 3</text>"));
    }

    #[test]
    fn subgraph_frames_never_overlap_and_nest() {
        let svg = svg_of("flowchart LR\nsubgraph One\n  a(\"cat\") --> b(\"dog\")\nend\nsubgraph Two\n  c(\"cat\") --> d(\"dog\")\nend\n");
        let frames = painted_rects(&svg, "#f8fafc");
        assert_eq!(frames.len(), 2, "{svg}");
        assert!(!rects_overlap(frames[0], frames[1]), "{frames:?}");
        // Nodes stay inside their own subgraph.
        let nodes = painted_rects(&svg, "#eff6ff");
        assert_eq!(nodes.len(), 4);
        for (k, node) in nodes.iter().enumerate() {
            let frame = frames[k / 2];
            assert!(rect_holds(frame, node.0, node.1) && rect_holds(frame, node.0 + node.2, node.1 + node.3), "{node:?} {frame:?}");
        }
        let nested = svg_of("flowchart TB\n  c1-->a2\n  subgraph TOP\n    subgraph B1\n      i1 --> f1\n    end\n    subgraph B2\n      i2 --> f2\n    end\n  end\n");
        let frames = painted_rects(&nested, "#f8fafc");
        assert_eq!(frames.len(), 3, "{nested}");
        let inside = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| a.0 > b.0 && a.1 > b.1 && a.0 + a.2 < b.0 + b.2 && a.1 + a.3 < b.1 + b.3;
        assert!(inside(frames[1], frames[0]) && inside(frames[2], frames[0]), "{frames:?}");
        assert!(!rects_overlap(frames[1], frames[2]));
        // `c1` is not in TOP.
        assert!(!rect_holds(frames[0], text_x(&nested, "c1"), text_y(&nested, "c1")), "{nested}");
    }

    #[test]
    fn markdown_edge_label_box_fits_bold_text_between_its_nodes() {
        let svg = svg_of("flowchart LR\n  c(\"x\") -- \"`Bold **edge label**`\" --> d(\"y\")\n");
        let label = painted_rects(&svg, "#ffffff").into_iter().find(|r| r.2 < 400.0).expect("label box");
        let bold = estimate_text_width("Bold ", 11.0) + estimate_text_width("edge label", 11.0) * BOLD_WIDTH;
        assert!(label.2 >= bold + 9.9, "{label:?} {bold}");
        // The label box sits in the gap between the two nodes.
        let nodes = painted_rects(&svg, "#eff6ff");
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|&n| !rects_overlap(n, label)), "{nodes:?} {label:?}");
        let widths = label_line_widths("`a **bbbbbbbb**`", 200.0, 11.0);
        assert!((widths[0] - (estimate_text_width("a ", 11.0) + estimate_text_width("bbbbbbbb", 11.0) * BOLD_WIDTH)).abs() < 0.01);
    }

    #[test]
    fn fa_icons_become_symbols() {
        assert_eq!(replace_fa_icons("fa:fa-ban forbidden"), "forbidden");
        assert_eq!(replace_fa_icons("A fa:fa-camera-retro perhaps?"), "A perhaps?");
        assert_eq!(replace_fa_icons("fab:fa-truck-bold a custom icon"), "a custom icon");
        assert_eq!(replace_fa_icons("sofa:fa-x and plain"), "sofa:fa-x and plain");
        let svg = svg_of("flowchart TD\n    B[\"fa:fa-twitter for peace\"]\n    B-->C[fa:fa-ban forbidden]\n");
        assert!(!svg.contains("fa:") && svg.contains(">forbidden<") && svg.contains(">for peace<"), "{svg}");
    }

    #[test]
    fn edges_to_a_subgraph_id_end_on_its_frame() {
        let svg = svg_of("flowchart LR\n  A --> TOP --> B\n  subgraph TOP\n    i1 --> f1\n  end\n");
        // `TOP` is the frame's title only, not a box of its own.
        assert!(svg.contains(r##"fill="#475569">TOP</text>"##), "{svg}");
        assert!(!svg.contains(r##"fill="#0f172a">TOP</text>"##), "{svg}");
        let frame = painted_rects(&svg, "#f8fafc")[0];
        let nodes = painted_rects(&svg, "#eff6ff");
        assert_eq!(nodes.len(), 4, "A, B, i1, f1: {svg}");
        // The edges from A and to B touch the frame's left and right sides.
        let ends: Vec<(f32, f32)> = fill_none_paths(&svg)
            .iter()
            .flat_map(|(d, _)| {
                let nums: Vec<f32> = d.split(|c: char| c.is_whitespace() || c == ',').filter_map(|t| t.parse().ok()).collect();
                [(nums[0], nums[1]), (nums[nums.len() - 2], nums[nums.len() - 1])]
            })
            .collect();
        assert!(ends.iter().any(|&(x, _)| (x - frame.0).abs() < 1.5), "{frame:?} {ends:?}");
        assert!(ends.iter().any(|&(x, _)| (x - (frame.0 + frame.2)).abs() < 1.5), "{frame:?} {ends:?}");
        assert!(text_x(&svg, "A") < frame.0 && text_x(&svg, "B") > frame.0 + frame.2);
    }

    #[test]
    fn collapsed_subgraph_is_one_box() {
        let svg = svg_of("flowchart TD\n    Start --> one\n    subgraph one [My Group]\n        A --> B\n        B --> C\n    end\n    one --> End\n    one@{ view: collapsed }\n");
        assert!(svg.contains(">My Group</text>") && svg.contains(r#"data-role="collapsed-subgraph""#), "{svg}");
        for hidden in [">A</text>", ">B</text>", ">C</text>"] {
            assert!(!svg.contains(hidden), "{hidden}: {svg}");
        }
        assert!(painted_rects(&svg, "#f8fafc").is_empty(), "no frame: {svg}");
        assert!(text_y(&svg, "Start") < text_y(&svg, "My Group") && text_y(&svg, "My Group") < text_y(&svg, "End"));
        // Edges into the closed subgraph go to the box.
        let svg = svg_of("flowchart TD\n    X --> A\n    subgraph one\n        A --> B\n    end\n    one@{ view: collapsed }\n");
        assert_eq!(fill_none_paths(&svg).len(), 1, "{svg}");
        assert!(!svg.contains(">A</text>") && svg.contains(">one</text>"), "{svg}");
    }

    #[test]
    fn icon_and_image_nodes() {
        let media = parse_node_media(r#"A@{ icon: "fa:user", form: "square", label: "User Icon", pos: "t", h: 60 }"#).unwrap();
        assert_eq!(media.kind, FlowMediaKind::Icon("fa:user".into()));
        assert!(media.label_on_top && media.h == 60.0 && media.w == 60.0 && media.form == "square");
        let svg = svg_of("flowchart TD\n    A@{ icon: \"fa:user\", form: \"square\", label: \"User Icon\", pos: \"t\", h: 60 }\n");
        let icon = tag_with(&svg, "data-icon=")[0];
        assert!(icon.contains(r#"data-icon="fa:user""#) && svg.contains("\u{263A}</text>"), "{svg}");
        assert!(text_y(&svg, "User Icon") < attr_f32(icon, "y").unwrap(), "label on top: {svg}");
        assert_eq!(attr_f32(icon, "height"), Some(60.0));
        let svg = svg_of("flowchart TD\n  A@{ img: \"https://mermaid.js.org/favicon.svg\", label: \"My image\", pos: \"b\", h: 60, constraint: \"on\" }\n");
        let img = tag_with(&svg, "data-img=")[0];
        assert!(img.contains("favicon.svg") && text_y(&svg, "My image") > attr_f32(img, "y").unwrap() + 60.0, "{svg}");
    }
}
