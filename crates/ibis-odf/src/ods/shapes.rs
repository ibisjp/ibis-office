// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing shapes in the cells of an `.ods` (ODF 1.3 chapter 10: `draw:custom-shape`, `draw:rect`, `draw:ellipse`,
//! `draw:line`) as the shapes of the sheet's drawing in the `.xlsx` made from it: the shape's kind, position and size,
//! fill, outline and text.

use std::collections::HashMap;

use roxmltree::Node;

use super::{DRAW, SVG, TEXT, attr, children, emu, esc};

/// The properties of the graphic, paragraph and text styles by name (attributes by local name), with the parent's.
pub(super) type Styles = HashMap<String, (Option<String>, HashMap<String, String>)>;

/// The properties of a style with the ones it takes from its parents.
fn props(styles: &Styles, name: &str) -> HashMap<String, String> {
    let mut chain = Vec::new();
    let mut at = Some(name.to_string());
    while let Some(n) = at {
        if chain.len() > 8 {
            break;
        }
        let Some((parent, p)) = styles.get(&n) else { break };
        chain.push(p);
        at = parent.clone();
    }
    let mut out = HashMap::new();
    for p in chain.into_iter().rev() {
        out.extend(p.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    out
}

pub(super) struct Shape {
    pub row: u32,
    pub col: u32,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    /// A line goes up to the right (from the lower left corner).
    pub flip_v: bool,
    pub flip_h: bool,
    /// `a:custGeom` of a shape whose `draw:enhanced-path` has equations (evaluated for the shape's size), instead of a preset.
    pub cust_geom: Option<String>,
    pub line_shape: bool,
    pub name: String,
    pub preset: String,
    pub fill: Option<u32>,
    /// Colour and width (EMU) of the outline, and whether it is dashed.
    pub outline: Option<(u32, i64, bool)>,
    pub text: Vec<String>,
    pub text_color: Option<u32>,
    pub text_size: Option<f64>,
    pub bold: bool,
    pub align: &'static str,
    pub anchor: &'static str,
    pub text_box: bool,
}

fn hex(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim().strip_prefix('#')?, 16).ok()
}

fn preset_of(ty: &str) -> String {
    if let Some(p) = ty.strip_prefix("ooxml-") {
        return p.to_string();
    }
    match ty {
        "rectangle" => "rect",
        "round-rectangle" => "roundRect",
        "ellipse" => "ellipse",
        "diamond" => "diamond",
        "isosceles-triangle" => "triangle",
        "right-triangle" => "rtTriangle",
        "parallelogram" => "parallelogram",
        "trapezoid" => "trapezoid",
        "hexagon" => "hexagon",
        "octagon" => "octagon",
        "pentagon" | "mso-spt56" => "pentagon",
        "right-arrow" => "rightArrow",
        "left-arrow" => "leftArrow",
        "up-arrow" => "upArrow",
        "down-arrow" => "downArrow",
        "star5" => "star5",
        "star4" => "star4",
        "cloud" => "cloud",
        "heart" => "heart",
        "smiley" => "smileyFace",
        _ => "rect",
    }
    .to_string()
}

fn text_lines(n: Node) -> (Vec<String>, Option<String>) {
    let mut span_style = None;
    let lines = children(n, TEXT, "p")
        .map(|p| {
            if span_style.is_none() {
                span_style = p.descendants().find(|d| d.is_element() && d.tag_name().name() == "span" && d.tag_name().namespace() == Some(TEXT)).and_then(|s| attr(s, TEXT, "style-name")).map(str::to_string);
            }
            p.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<String>()
        })
        .collect();
    (lines, span_style)
}

/// The shapes among the children of the cell at (`row`, `col`).
pub(super) fn cell_shapes(cell: Node, row: u32, col: u32, styles: &Styles) -> Vec<Shape> {
    let mut out = Vec::new();
    for n in cell.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(DRAW)) {
        let kind = n.tag_name().name();
        // A frame holds a text box; the others are the shapes themselves.
        let (shape_node, text_box) = match kind {
            "custom-shape" | "rect" | "ellipse" | "circle" | "line" => (n, false),
            "frame" => match children(n, DRAW, "text-box").next() {
                Some(tb) => (tb, true),
                None => continue,
            },
            _ => continue,
        };
        let len = |node: Node, name: &str| attr(node, SVG, name).and_then(emu);
        let (x, y, w, h, flip_v) = if kind == "line" {
            let (Some(x1), Some(y1), Some(x2), Some(y2)) = (len(n, "x1"), len(n, "y1"), len(n, "x2"), len(n, "y2")) else { continue };
            let flip = (x2 < x1) != (y2 < y1);
            (x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs(), flip)
        } else {
            let (Some(w), Some(h)) = (len(n, "width"), len(n, "height")) else { continue };
            (len(n, "x").unwrap_or(0), len(n, "y").unwrap_or(0), w, h, false)
        };
        let style = attr(n, DRAW, "style-name").map(|s| props(styles, s)).unwrap_or_default();
        let (lines, span_style) = text_lines(shape_node);
        let span = span_style.map(|s| props(styles, &s)).unwrap_or_default();
        let get = |k: &str| span.get(k).or_else(|| style.get(k));
        let geometry = children(n, DRAW, "enhanced-geometry").next().and_then(|g| attr(g, DRAW, "type")).map(preset_of);
        let eg = children(n, DRAW, "enhanced-geometry").next();
        let cust_geom = if kind == "custom-shape" {
            eg.filter(|g| attr(*g, DRAW, "enhanced-path").is_some_and(|p| p.contains('?') || p.contains('$')))
                .and_then(|g| ibis_geometry(g, (w as f64 / 360.0, h as f64 / 360.0)))
        } else {
            None
        };
        let mirror = |k: &str| eg.is_some_and(|g| attr(g, DRAW, k) == Some("true"));
        let (flip_h, flip_v) = (mirror("mirror-horizontal"), flip_v ^ mirror("mirror-vertical"));
        let preset = match (kind, geometry) {
            ("line", _) => "line".to_string(),
            ("ellipse" | "circle", _) => "ellipse".to_string(),
            (_, Some(g)) => g,
            ("rect", _) if attr(n, DRAW, "corner-radius").is_some() => "roundRect".to_string(),
            _ => "rect".to_string(),
        };
        let fill = if style.get("fill").map(String::as_str) == Some("solid") { style.get("fill-color").and_then(|c| hex(c)) } else { None };
        let outline = match style.get("stroke").map(String::as_str) {
            Some("solid" | "dash") => Some((style.get("stroke-color").and_then(|c| hex(c)).unwrap_or(0), style.get("stroke-width").and_then(|w| emu(w)).unwrap_or(12700).max(1), style.get("stroke").map(String::as_str) == Some("dash"))),
            _ => None,
        };
        out.push(Shape {
            row,
            col,
            x,
            y,
            width: w,
            height: h,
            flip_v,
            flip_h,
            cust_geom,
            line_shape: kind == "line",
            name: attr(n, DRAW, "name").unwrap_or("").to_string(),
            preset,
            fill,
            outline,
            text: if lines.iter().all(String::is_empty) { Vec::new() } else { lines },
            text_color: get("color").and_then(|c| hex(c)),
            text_size: get("font-size").and_then(|s| super::points(s)),
            bold: get("font-weight").map(String::as_str) == Some("bold"),
            align: match style.get("text-align").map(String::as_str) {
                Some("center") => "ctr",
                Some("end" | "right") => "r",
                _ => "l",
            },
            anchor: match style.get("textarea-vertical-align").map(String::as_str) {
                Some("middle") => "ctr",
                Some("bottom") => "b",
                _ => "t",
            },
            text_box,
        });
    }
    out
}

/// The anchor of a shape in the drawing part: `id` is its number.
pub(super) fn anchor_xml(s: &Shape, id: usize) -> String {
    let from = format!("<xdr:from><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:from>", s.col, s.x.max(0), s.row, s.y.max(0));
    let name = if s.name.is_empty() { format!("Shape {id}") } else { s.name.clone() };
    let ln = match s.outline {
        Some((c, w, dash)) => format!("<a:ln w=\"{w}\"><a:solidFill><a:srgbClr val=\"{c:06X}\"/></a:solidFill>{}</a:ln>", if dash { "<a:prstDash val=\"dash\"/>" } else { "" }),
        None => "<a:ln><a:noFill/></a:ln>".to_string(),
    };
    let xfrm = format!("<a:xfrm{}{}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>", if s.flip_h { " flipH=\"1\"" } else { "" }, if s.flip_v { " flipV=\"1\"" } else { "" }, s.width, s.height);
    let body = if s.line_shape {
        format!("<xdr:cxnSp macro=\"\"><xdr:nvCxnSpPr><xdr:cNvPr id=\"{id}\" name=\"{}\"/><xdr:cNvCxnSpPr/></xdr:nvCxnSpPr><xdr:spPr>{xfrm}<a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom>{ln}</xdr:spPr></xdr:cxnSp>", esc(&name))
    } else {
        let fill = match s.fill {
            Some(c) => format!("<a:solidFill><a:srgbClr val=\"{c:06X}\"/></a:solidFill>"),
            None => "<a:noFill/>".to_string(),
        };
        let geometry = match &s.cust_geom {
            Some(g) => g.clone(),
            None => format!("<a:prstGeom prst=\"{}\"><a:avLst/></a:prstGeom>", esc(&s.preset)),
        };
        let text = if s.text.is_empty() {
            String::new()
        } else {
            let run_props = format!("<a:rPr lang=\"en-US\"{}{}>{}</a:rPr>", s.text_size.map(|p| format!(" sz=\"{}\"", (p * 100.0).round() as i64)).unwrap_or_default(), if s.bold { " b=\"1\"" } else { "" }, s.text_color.map(|c| format!("<a:solidFill><a:srgbClr val=\"{c:06X}\"/></a:solidFill>")).unwrap_or_default());
            let paras: String = s.text.iter().map(|l| format!("<a:p><a:pPr algn=\"{}\"/><a:r>{run_props}<a:t>{}</a:t></a:r></a:p>", s.align, esc(l))).collect();
            format!("<xdr:txBody><a:bodyPr vertOverflow=\"clip\" wrap=\"square\" rtlCol=\"0\" anchor=\"{}\"/><a:lstStyle/>{paras}</xdr:txBody>", s.anchor)
        };
        format!("<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"{}\"/><xdr:cNvSpPr{}/></xdr:nvSpPr><xdr:spPr>{xfrm}{}{fill}{ln}</xdr:spPr>{text}</xdr:sp>", esc(&name), if s.text_box { " txBox=\"1\"" } else { "" }, geometry)
    };
    format!("<xdr:oneCellAnchor>{from}<xdr:ext cx=\"{}\" cy=\"{}\"/>{body}<xdr:clientData/></xdr:oneCellAnchor>", s.width, s.height)
}

/// The `a:custGeom` of a `draw:enhanced-geometry` with equations (shared with the other ODF readers), for a shape of `size`
/// (1/100 mm).
fn ibis_geometry(eg: Node, size: (f64, f64)) -> Option<String> {
    let geo = crate::enhanced_geometry::evaluate_in(eg, size, true)?;
    Some(format!("<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst>{}</a:pathLst></a:custGeom>", geo.paths))
}
