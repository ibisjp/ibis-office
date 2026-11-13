// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The drawing objects of an OpenDocument presentation ([ODF 1.3] part 1, chapter 10): frames (text boxes, pictures, tables),
//! rectangles, ellipses, lines, connectors, polygons, paths, custom shapes and groups, with their fills, lines, shadows and
//! text, as the DrawingML shapes of a slide.

use std::fmt::Write as _;

use roxmltree::Node;

use crate::odp::Cx;
use crate::odp_style::{DRAW, FO, LOEXT, PRESENTATION, STYLE, SVG, TABLE, XLINK, attr, children, color, emu, is, percent};
use crate::odp_text::{Body, esc, has_text, paragraphs};

/// Where an object is: its box in EMU, rotation (60000ths of a degree, clockwise) and flips.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Geom {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) w: i64,
    pub(crate) h: i64,
    pub(crate) rot: i64,
    pub(crate) flip_h: bool,
    pub(crate) flip_v: bool,
}

/// The value of a number list "a b c" in a transform.
fn numbers(s: &str) -> Vec<f64> {
    s.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).filter_map(|t| t.trim_end_matches(|c: char| c.is_ascii_alphabetic()).parse::<f64>().ok()).collect()
}

/// The `draw:transform` as (rotation in radians counterclockwise, translation in EMU).
fn transform(text: &str) -> (f64, f64, f64) {
    let (mut angle, mut tx, mut ty) = (0.0, 0.0, 0.0);
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(|c: char| c.is_whitespace()).to_string();
        let Some(close) = rest[open..].find(')') else { break };
        let args = &rest[open + 1..open + close];
        match name.as_str() {
            "rotate" => angle = numbers(args).first().copied().unwrap_or(0.0),
            "translate" => {
                let parts: Vec<&str> = args.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).collect();
                tx = parts.first().and_then(|p| emu(p)).unwrap_or(0) as f64;
                ty = parts.get(1).and_then(|p| emu(p)).unwrap_or(0) as f64;
            }
            _ => {}
        }
        rest = &rest[open + close + 1..];
    }
    (angle, tx, ty)
}

pub(crate) fn geom_of(n: Node) -> Option<Geom> {
    let len = |local: &str| attr(n, SVG, local).and_then(emu);
    let (w, h) = (len("width")?, len("height")?);
    let mut g = Geom { x: len("x").unwrap_or(0), y: len("y").unwrap_or(0), w, h, ..Default::default() };
    if let Some(t) = attr(n, DRAW, "transform") {
        let (angle, tx, ty) = transform(t);
        // The object turns about its own corner, then moves: its centre is where the turned half-diagonal ends.
        let (cos, sin) = (angle.cos(), angle.sin());
        let (hw, hh) = (w as f64 / 2.0, h as f64 / 2.0);
        let cx = tx + hw * cos + hh * sin;
        let cy = ty - hw * sin + hh * cos;
        g.x = (cx - hw).round() as i64;
        g.y = (cy - hh).round() as i64;
        let degrees = (-angle.to_degrees()).rem_euclid(360.0);
        g.rot = (degrees * 60_000.0).round() as i64 % 21_600_000;
    }
    Some(g)
}

fn xfrm(g: &Geom, tag: &str) -> String {
    let mut attrs = String::new();
    if g.rot != 0 {
        let _ = write!(attrs, r#" rot="{}""#, g.rot);
    }
    if g.flip_h {
        attrs += r#" flipH="1""#;
    }
    if g.flip_v {
        attrs += r#" flipV="1""#;
    }
    format!(r#"<{tag}{attrs}><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></{tag}>"#, g.x, g.y, g.w.max(0), g.h.max(0))
}

/// The styles of an object, most specific last: its graphic or presentation style.
fn style_chain(n: Node) -> Vec<(&'static str, String)> {
    let mut chain = Vec::new();
    if let Some(s) = attr(n, PRESENTATION, "style-name") {
        chain.push(("presentation", s.to_string()));
    }
    if let Some(s) = attr(n, DRAW, "style-name") {
        chain.push(("graphic", s.to_string()));
    }
    chain
}

/// The chain for the text of an object: its styles, then the paragraph style it names for its text.
fn text_chain(n: Node) -> Vec<(&'static str, String)> {
    let mut chain = style_chain(n);
    if let Some(s) = attr(n, DRAW, "text-style-name") {
        chain.push(("paragraph", s.to_string()));
    }
    chain
}

fn names<'a>(chain: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    chain.iter().map(|(f, n)| (*f, n.as_str())).collect()
}

fn with_alpha(c: &str, alpha: Option<f64>) -> String {
    match alpha.filter(|a| *a < 0.999) {
        Some(a) => format!(r#"<a:srgbClr val="{c}"><a:alpha val="{}"/></a:srgbClr>"#, (a * 100_000.0).round() as i64),
        None => format!(r#"<a:srgbClr val="{c}"/>"#),
    }
}

/// A colour scaled by an intensity (a gradient's start and end intensities).
fn intensity(c: &str, factor: f64) -> String {
    if (factor - 1.0).abs() < 1e-6 {
        return c.to_string();
    }
    let part = |i: usize| u8::from_str_radix(&c[i..i + 2], 16).unwrap_or(0);
    let scale = |v: u8| ((f64::from(v) * factor).round().clamp(0.0, 255.0)) as u8;
    format!("{:02X}{:02X}{:02X}", scale(part(0)), scale(part(2)), scale(part(4)))
}

/// A gradient fill from the gradient object `name`.
fn gradient(cx: &Cx, name: &str, alpha: Option<f64>, shape_rotation: f64) -> Option<String> {
    let g = *cx.st.gradients.get(name)?;
    let style = attr(g, DRAW, "style").unwrap_or("linear");
    let start = attr(g, DRAW, "start-color").and_then(color)?;
    let end = attr(g, DRAW, "end-color").and_then(color)?;
    let start = intensity(&start, attr(g, DRAW, "start-intensity").and_then(percent).unwrap_or(1.0));
    let end = intensity(&end, attr(g, DRAW, "end-intensity").and_then(percent).unwrap_or(1.0));
    let border = attr(g, DRAW, "border").and_then(percent).unwrap_or(0.0).clamp(0.0, 0.99);
    // The stops, from the start colour at the start; the border keeps the start colour for a part.
    let mut stops: Vec<(f64, String)> = Vec::new();
    let multi: Vec<(f64, String)> = g
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "gradient-stop")
        .filter_map(|c| Some((attr(c, SVG, "offset").and_then(|o| o.parse::<f64>().ok())?, attr(c, SVG, "stop-color").and_then(color)?)))
        .collect();
    if multi.len() >= 2 {
        stops = multi;
    } else {
        if border > 0.0 {
            stops.push((0.0, start.clone()));
            stops.push((border, start.clone()));
        } else {
            stops.push((0.0, start.clone()));
        }
        stops.push((1.0, end.clone()));
    }
    let angle = attr(g, DRAW, "angle").map(|a| a.trim_end_matches("deg").parse::<f64>().unwrap_or(0.0)).map(|a| if attr(g, DRAW, "angle").is_some_and(|t| t.ends_with("deg")) { a } else { a / 10.0 }).unwrap_or(0.0);
    let gs = |stops: &[(f64, String)]| -> String { stops.iter().map(|(o, c)| format!(r#"<a:gs pos="{}">{}</a:gs>"#, (o * 100_000.0).round() as i64, with_alpha(c, alpha))).collect() };
    // (The gradient turns with its shape in DrawingML, but is given on the page in ODF.)
    let ooxml_angle = ((90.0 - angle - shape_rotation).rem_euclid(360.0) * 60_000.0).round() as i64;
    Some(match style {
        "axial" => {
            // The start colour is at the middle, the end colour at both edges.
            let mut mirrored: Vec<(f64, String)> = stops.iter().rev().map(|(o, c)| ((1.0 - o) / 2.0, c.clone())).collect();
            mirrored.extend(stops.iter().map(|(o, c)| (0.5 + o / 2.0, c.clone())));
            mirrored.reverse();
            mirrored.sort_by(|a, b| a.0.total_cmp(&b.0));
            let flipped: Vec<(f64, String)> = mirrored.iter().map(|(o, c)| (*o, c.clone())).collect();
            // (Written so that position 0 and 1 are the end colour, 0.5 the start colour.)
            let axial: Vec<(f64, String)> = vec![(0.0, end.clone()), (0.5, start.clone()), (1.0, end.clone())];
            let _ = flipped;
            format!(r#"<a:gradFill rotWithShape="1"><a:gsLst>{}</a:gsLst><a:lin ang="{ooxml_angle}" scaled="0"/></a:gradFill>"#, gs(&axial))
        }
        "radial" | "ellipsoid" | "square" | "rectangular" => {
            let (cx_, cy_) = (attr(g, DRAW, "cx").and_then(percent).unwrap_or(0.5), attr(g, DRAW, "cy").and_then(percent).unwrap_or(0.5));
            // The end colour at the centre, the start colour at the edge.
            let mut from_centre: Vec<(f64, String)> = stops.iter().rev().map(|(o, c)| (1.0 - o, c.clone())).collect();
            from_centre.sort_by(|a, b| a.0.total_cmp(&b.0));
            let path = if matches!(style, "square" | "rectangular") { "rect" } else { "circle" };
            let (l, t) = ((cx_ * 100_000.0).round() as i64, (cy_ * 100_000.0).round() as i64);
            format!(r#"<a:gradFill rotWithShape="1"><a:gsLst>{}</a:gsLst><a:path path="{path}"><a:fillToRect l="{l}" t="{t}" r="{}" b="{}"/></a:path></a:gradFill>"#, gs(&from_centre), 100_000 - l, 100_000 - t)
        }
        _ => format!(r#"<a:gradFill rotWithShape="1"><a:gsLst>{}</a:gsLst><a:lin ang="{ooxml_angle}" scaled="0"/></a:gradFill>"#, gs(&stops)),
    })
}

/// The fill an object's style gives: `<a:noFill/>`, a solid, gradient or picture fill; empty when the style says nothing.
pub(crate) fn fill_of(cx: &mut Cx, chain: &[(&'static str, String)]) -> String {
    let get = |ns: &str, local: &str| cx.st.prop_in(&names(chain), ns, local);
    let alpha = get(DRAW, "opacity").and_then(percent);
    match get(DRAW, "fill") {
        Some("none") => "<a:noFill/>".to_string(),
        Some("solid") => match get(DRAW, "fill-color").and_then(color) {
            Some(c) => format!("<a:solidFill>{}</a:solidFill>", with_alpha(&c, alpha)),
            None => String::new(),
        },
        Some("gradient") => get(DRAW, "fill-gradient-name").and_then(|n| gradient(cx, n, alpha, cx.shape_rotation)).unwrap_or_default(),
        Some("bitmap") => {
            let Some(image) = get(DRAW, "fill-image-name").and_then(|n| cx.st.images.get(n).copied()) else { return String::new() };
            let Some(rid) = attr(image, XLINK, "href").and_then(|h| cx.image_rel(h)) else { return String::new() };
            let repeat = get(DRAW, "fill-image-rendering-style").unwrap_or("stretch");
            if repeat == "repeat" {
                format!(r#"<a:blipFill rotWithShape="1"><a:blip r:embed="{rid}"/><a:tile tx="0" ty="0" sx="100000" sy="100000" flip="none" algn="tl"/></a:blipFill>"#)
            } else {
                format!(r#"<a:blipFill rotWithShape="1"><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"#)
            }
        }
        Some("hatch") => {
            let hatch = get(DRAW, "fill-hatch-name").and_then(|n| cx.st.hatches.get(n).copied());
            let fg = hatch.and_then(|h| attr(h, DRAW, "color")).and_then(color).unwrap_or_else(|| "000000".to_string());
            let bg = if get(DRAW, "fill-hatch-solid") == Some("true") { get(DRAW, "fill-color").and_then(color).unwrap_or_else(|| "FFFFFF".to_string()) } else { "FFFFFF".to_string() };
            let prst = match hatch.and_then(|h| attr(h, DRAW, "style")) {
                Some("double") | Some("triple") => "ltGrid",
                _ => match hatch.and_then(|h| attr(h, DRAW, "rotation")).and_then(|r| r.parse::<f64>().ok()).map(|r| r.rem_euclid(1800.0)) {
                    Some(r) if (r - 900.0).abs() < 1.0 => "ltVert",
                    Some(r) if r < 1.0 => "ltHorz",
                    Some(r) if r < 900.0 => "ltUpDiag",
                    _ => "ltDnDiag",
                },
            };
            format!(r#"<a:pattFill prst="{prst}"><a:fgClr><a:srgbClr val="{fg}"/></a:fgClr><a:bgClr><a:srgbClr val="{bg}"/></a:bgClr></a:pattFill>"#)
        }
        _ => String::new(),
    }
}

/// The line (`a:ln`) an object's style gives; empty when the style says nothing.
fn line_xml(cx: &mut Cx, chain: &[(&'static str, String)]) -> String {
    let get = |ns: &str, local: &str| cx.st.prop_in(&names(chain), ns, local);
    let stroke = get(DRAW, "stroke");
    match stroke {
        None => return String::new(),
        Some("none") => return "<a:ln><a:noFill/></a:ln>".to_string(),
        _ => {}
    }
    let width = get(SVG, "stroke-width").and_then(emu).unwrap_or(0);
    let c = get(SVG, "stroke-color").and_then(color).unwrap_or_else(|| "000000".to_string());
    let alpha = get(SVG, "stroke-opacity").and_then(percent);
    let mut attrs = String::new();
    if width > 0 {
        let _ = write!(attrs, r#" w="{width}""#);
    }
    match get(SVG, "stroke-linecap") {
        Some("round") => attrs += r#" cap="rnd""#,
        Some("square") => attrs += r#" cap="sq""#,
        _ => {}
    }
    let mut inner = format!("<a:solidFill>{}</a:solidFill>", with_alpha(&c, alpha));
    if stroke == Some("dash") {
        if let Some(d) = get(DRAW, "stroke-dash").and_then(|n| cx.st.dashes.get(n).copied()) {
            let ratio = |text: Option<&str>| -> i64 {
                let Some(t) = text else { return 0 };
                match percent(t) {
                    Some(p) => (p * 100_000.0).round() as i64,
                    None => emu(t).filter(|_| width > 0).map_or(0, |l| (l as f64 / width as f64 * 100_000.0).round() as i64),
                }
            };
            // Dots of two lengths with a distance between: the dash pattern, in the line's widths.
            let (n1, n2) = (attr(d, DRAW, "dots1").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1), attr(d, DRAW, "dots2").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0));
            let (l1, l2) = (ratio(attr(d, DRAW, "dots1-length")), ratio(attr(d, DRAW, "dots2-length")));
            let distance = ratio(attr(d, DRAW, "distance"));
            // (A dot without a length is one line width.)
            let (l1, l2) = (if l1 == 0 { 100_000 } else { l1 }, if l2 == 0 { 100_000 } else { l2 });
            let distance = if distance == 0 { 100_000 } else { distance };
            let mut pattern = String::new();
            for _ in 0..n1.min(16) {
                let _ = write!(pattern, r#"<a:ds d="{l1}" sp="{distance}"/>"#);
            }
            for _ in 0..n2.min(16) {
                let _ = write!(pattern, r#"<a:ds d="{l2}" sp="{distance}"/>"#);
            }
            // A simple pattern is a preset dash; others stay as they are.
            let ratio_of = |on: i64, off: i64| on as f64 / off.max(1) as f64;
            let preset = if n2 == 0 && n1 == 1 {
                let r = ratio_of(l1, distance);
                [("lgDash", 8.0 / 3.0), ("dash", 4.0 / 3.0), ("sysDash", 3.0), ("sysDot", 1.0), ("dot", 1.0 / 3.0)].into_iter().min_by(|a, b| (a.1 - r).abs().total_cmp(&(b.1 - r).abs())).map(|p| p.0)
            } else if n2 == 1 && n1 == 1 {
                Some(if l1 as f64 / 100_000.0 >= 6.0 { "lgDashDot" } else { "dashDot" })
            } else {
                None
            };
            match preset {
                Some(p) => {
                    let _ = write!(inner, r#"<a:prstDash val="{p}"/>"#);
                }
                None => {
                    let _ = write!(inner, "<a:custDash>{pattern}</a:custDash>");
                }
            }
        } else {
            inner += r#"<a:prstDash val="dash"/>"#;
        }
    }
    match get(DRAW, "stroke-linejoin") {
        Some("round") => inner += "<a:round/>",
        Some("bevel") => inner += "<a:bevel/>",
        Some("miter") => inner += r#"<a:miter lim="800000"/>"#,
        _ => {}
    }
    for (kind, tag) in [("start", "headEnd"), ("end", "tailEnd")] {
        if let Some(marker) = get(DRAW, &format!("marker-{kind}")) {
            let lower = marker.to_lowercase();
            let t = if lower.contains("circle") || lower.contains("oval") {
                "oval"
            } else if lower.contains("square") || lower.contains("diamond") {
                "diamond"
            } else if lower.contains("line_20_arrow") || lower.contains("line arrow") {
                "arrow"
            } else if lower.contains("arrow") {
                "triangle"
            } else {
                "none"
            };
            let marker_width = get(DRAW, &format!("marker-{kind}-width")).and_then(emu).unwrap_or(0);
            let size = if width > 0 { marker_width as f64 / width as f64 } else { 3.0 };
            let s = if size < 2.5 { "sm" } else if size < 5.0 { "med" } else { "lg" };
            if t != "none" {
                let _ = write!(inner, r#"<a:{tag} type="{t}" w="{s}" len="{s}"/>"#);
            }
        }
    }
    format!("<a:ln{attrs}>{inner}</a:ln>")
}

/// The shadow of an object (`a:effectLst`).
fn shadow_xml(cx: &Cx, chain: &[(&'static str, String)]) -> String {
    let get = |ns: &str, local: &str| cx.st.prop_in(&names(chain), ns, local);
    if get(DRAW, "shadow") != Some("visible") {
        return String::new();
    }
    let (dx, dy) = (get(DRAW, "shadow-offset-x").and_then(emu).unwrap_or(0) as f64, get(DRAW, "shadow-offset-y").and_then(emu).unwrap_or(0) as f64);
    let c = get(DRAW, "shadow-color").and_then(color).unwrap_or_else(|| "808080".to_string());
    let alpha = get(DRAW, "shadow-opacity").and_then(percent);
    let blur = get(LOEXT, "shadow-blur").and_then(emu).unwrap_or(0);
    let dist = dx.hypot(dy).round() as i64;
    let dir = (dy.atan2(dx).to_degrees().rem_euclid(360.0) * 60_000.0).round() as i64;
    format!(r#"<a:effectLst><a:outerShdw blurRad="{blur}" dist="{dist}" dir="{dir}" algn="tl" rotWithShape="0">{}</a:outerShdw></a:effectLst>"#, with_alpha(&c, alpha))
}

/// The text body properties (`a:bodyPr`) of an object.
fn body_pr(cx: &Cx, chain: &[(&'static str, String)]) -> String {
    let get = |ns: &str, local: &str| cx.st.prop_in(&names(chain), ns, local);
    let mut attrs = String::new();
    for (side, name) in [("left", "lIns"), ("top", "tIns"), ("right", "rIns"), ("bottom", "bIns")] {
        if let Some(v) = get(FO, &format!("padding-{side}")).and_then(emu) {
            let _ = write!(attrs, r#" {name}="{v}""#);
        }
    }
    let anchor = match get(DRAW, "textarea-vertical-align") {
        Some("middle") => Some("ctr"),
        Some("bottom") => Some("b"),
        Some("top") => Some("t"),
        _ => None,
    };
    if let Some(a) = anchor {
        let _ = write!(attrs, r#" anchor="{a}""#);
    }
    if get(DRAW, "textarea-horizontal-align") == Some("center") {
        attrs += r#" anchorCtr="1""#;
    }
    if get(FO, "wrap-option") == Some("no-wrap") {
        attrs += r#" wrap="none""#;
    }
    match get(STYLE, "writing-mode") {
        Some("tb-rl" | "tb-lr") => attrs += r#" vert="eaVert""#,
        _ => {}
    }
    attrs += r#" rtlCol="0""#;
    let fit = if get(STYLE, "shrink-to-fit") == Some("true") || get(DRAW, "fit-to-size") == Some("true") {
        "<a:normAutofit/>"
    } else if get(DRAW, "auto-grow-height") == Some("true") {
        "<a:spAutoFit/>"
    } else {
        "<a:noAutofit/>"
    };
    format!("<a:bodyPr{attrs}>{fit}</a:bodyPr>")
}

/// The preset shape of a custom shape's type.
fn preset_of(kind: &str) -> Option<String> {
    if let Some(name) = kind.strip_prefix("ooxml-") {
        return crate::geometry::preset(name).is_some().then(|| name.to_string());
    }
    if let Some(n) = kind.strip_prefix("mso-spt").and_then(|n| n.parse::<u16>().ok()) {
        return crate::officeart_xml::preset(n).map(str::to_string);
    }
    let name = match kind {
        "rectangle" | "quadrat" => "rect",
        "round-rectangle" | "round-quadrat" => "roundRect",
        "ellipse" | "circle" => "ellipse",
        "diamond" => "diamond",
        "isosceles-triangle" | "triangle" => "triangle",
        "right-triangle" => "rtTriangle",
        "parallelogram" => "parallelogram",
        "trapezoid" => "trapezoid",
        "hexagon" => "hexagon",
        "octagon" => "octagon",
        "cross" => "plus",
        "ring" => "donut",
        "can" => "can",
        "cube" => "cube",
        "paper" => "foldedCorner",
        "heart" => "heart",
        "sun" => "sun",
        "moon" => "moon",
        "smiley" => "smileyFace",
        "lightning-bolt" => "lightningBolt",
        "star4" | "4-point-star" => "star4",
        "star5" | "5-point-star" => "star5",
        "star6" | "6-point-star" => "star6",
        "star8" | "8-point-star" => "star8",
        "star12" | "12-point-star" => "star12",
        "star24" | "24-point-star" => "star24",
        "right-arrow" => "rightArrow",
        "left-arrow" => "leftArrow",
        "up-arrow" => "upArrow",
        "down-arrow" => "downArrow",
        "left-right-arrow" => "leftRightArrow",
        "up-down-arrow" => "upDownArrow",
        "bent-arrow" => "bentArrow",
        "u-turn-arrow" => "uturnArrow",
        "circular-arrow" => "circularArrow",
        "notched-right-arrow" => "notchedRightArrow",
        "pentagon" | "regular-pentagon" => "pentagon",
        "chevron" => "chevron",
        "ellipse-callout" | "round-callout" => "wedgeEllipseCallout",
        "rectangular-callout" => "wedgeRectCallout",
        "round-rectangular-callout" => "wedgeRoundRectCallout",
        "cloud" => "cloud",
        "frame" => "frame",
        "block-arc" => "blockArc",
        "flowchart-process" => "flowChartProcess",
        "flowchart-alternate-process" => "flowChartAlternateProcess",
        "flowchart-decision" => "flowChartDecision",
        "flowchart-data" => "flowChartInputOutput",
        "flowchart-predefined-process" => "flowChartPredefinedProcess",
        "flowchart-document" => "flowChartDocument",
        "flowchart-terminator" => "flowChartTerminator",
        "flowchart-connector" => "flowChartConnector",
        "flowchart-extract" => "flowChartExtract",
        "flowchart-merge" => "flowChartMerge",
        "flowchart-card" => "flowChartPunchedCard",
        "flowchart-delay" => "flowChartDelay",
        "flowchart-display" => "flowChartDisplay",
        "flowchart-sort" => "flowChartSort",
        "flowchart-summing-junction" => "flowChartSummingJunction",
        "flowchart-or" => "flowChartOr",
        "flowchart-collate" => "flowChartCollate",
        "flowchart-stored-data" => "flowChartOnlineStorage",
        "flowchart-internal-storage" => "flowChartInternalStorage",
        "flowchart-sequential-access" => "flowChartMagneticTape",
        "flowchart-direct-access-storage" => "flowChartMagneticDisk",
        "flowchart-manual-input" => "flowChartManualInput",
        "flowchart-manual-operation" => "flowChartManualOperation",
        "flowchart-preparation" => "flowChartPreparation",
        "flowchart-off-page-connector" => "flowChartOffpageConnector",
        "flowchart-multidocument" => "flowChartMultidocument",
        _ => return None,
    };
    Some(name.to_string())
}

fn avlst(adjust: &[(&str, i64)]) -> String {
    if adjust.is_empty() {
        return "<a:avLst/>".to_string();
    }
    let gd: String = adjust.iter().map(|(n, v)| format!(r#"<a:gd name="{n}" fmla="val {v}"/>"#)).collect();
    format!("<a:avLst>{gd}</a:avLst>")
}

/// A path in the custom geometry: its width and height (the coordinate space) and the commands.
fn cust_geom(w: i64, h: i64, commands: &str, fill: bool, stroke: bool) -> String {
    let attrs = format!("{}{}", if fill { "" } else { r#" fill="none""# }, if stroke { "" } else { r#" stroke="0""# });
    format!(r#"<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/><a:pathLst><a:path w="{w}" h="{h}"{attrs}>{commands}</a:path></a:pathLst></a:custGeom>"#)
}

/// The points of `draw:points` ("x,y x,y ...") as integers in the view box's units.
fn point_list(text: &str) -> Vec<(i64, i64)> {
    text.split_whitespace().filter_map(|p| {
        let (x, y) = p.split_once(',')?;
        Some((x.parse::<f64>().ok()?.round() as i64, y.parse::<f64>().ok()?.round() as i64))
    }).collect()
}

fn view_box(n: Node) -> Option<(i64, i64, i64, i64)> {
    let v: Vec<i64> = attr(n, SVG, "viewBox")?.split_whitespace().filter_map(|t| t.parse::<f64>().ok()).map(|f| f.round() as i64).collect();
    (v.len() == 4 && v[2] > 0 && v[3] > 0).then(|| (v[0], v[1], v[2], v[3]))
}

/// The commands of an SVG path (`svg:d`) as DrawingML path commands, in the view box's units shifted to start at 0.
fn svg_path(d: &str, origin: (i64, i64)) -> String {
    let mut out = String::new();
    let tokens: Vec<String> = {
        let mut t = Vec::new();
        let mut cur = String::new();
        for c in d.chars() {
            if c.is_ascii_alphabetic() {
                if !cur.trim().is_empty() {
                    t.push(cur.trim().to_string());
                }
                cur = c.to_string();
            } else {
                cur.push(c);
            }
        }
        if !cur.trim().is_empty() {
            t.push(cur.trim().to_string());
        }
        t
    };
    let (mut cx_, mut cy_) = (0.0f64, 0.0f64);
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    let pt = |x: f64, y: f64| format!(r#"<a:pt x="{}" y="{}"/>"#, (x - origin.0 as f64).round() as i64, (y - origin.1 as f64).round() as i64);
    for token in tokens {
        let cmd = token.chars().next().unwrap_or(' ');
        let nums = numbers(&token[1..].replace('-', " -").replace("e -", "e-"));
        let rel = cmd.is_ascii_lowercase();
        match cmd.to_ascii_uppercase() {
            'M' => {
                for (i, c) in nums.chunks(2).enumerate().filter(|(_, c)| c.len() == 2) {
                    let (x, y) = if rel { (cx_ + c[0], cy_ + c[1]) } else { (c[0], c[1]) };
                    if i == 0 {
                        let _ = write!(out, "<a:moveTo>{}</a:moveTo>", pt(x, y));
                        (sx, sy) = (x, y);
                    } else {
                        let _ = write!(out, "<a:lnTo>{}</a:lnTo>", pt(x, y));
                    }
                    (cx_, cy_) = (x, y);
                }
            }
            'L' => {
                for c in nums.chunks(2).filter(|c| c.len() == 2) {
                    let (x, y) = if rel { (cx_ + c[0], cy_ + c[1]) } else { (c[0], c[1]) };
                    let _ = write!(out, "<a:lnTo>{}</a:lnTo>", pt(x, y));
                    (cx_, cy_) = (x, y);
                }
            }
            'H' => {
                for v in &nums {
                    let x = if rel { cx_ + v } else { *v };
                    let _ = write!(out, "<a:lnTo>{}</a:lnTo>", pt(x, cy_));
                    cx_ = x;
                }
            }
            'V' => {
                for v in &nums {
                    let y = if rel { cy_ + v } else { *v };
                    let _ = write!(out, "<a:lnTo>{}</a:lnTo>", pt(cx_, y));
                    cy_ = y;
                }
            }
            'C' => {
                for c in nums.chunks(6).filter(|c| c.len() == 6) {
                    let p: Vec<(f64, f64)> = (0..3).map(|i| if rel { (cx_ + c[i * 2], cy_ + c[i * 2 + 1]) } else { (c[i * 2], c[i * 2 + 1]) }).collect();
                    let _ = write!(out, "<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(p[0].0, p[0].1), pt(p[1].0, p[1].1), pt(p[2].0, p[2].1));
                    (cx_, cy_) = p[2];
                }
            }
            'Q' => {
                for c in nums.chunks(4).filter(|c| c.len() == 4) {
                    let p: Vec<(f64, f64)> = (0..2).map(|i| if rel { (cx_ + c[i * 2], cy_ + c[i * 2 + 1]) } else { (c[i * 2], c[i * 2 + 1]) }).collect();
                    let _ = write!(out, "<a:quadBezTo>{}{}</a:quadBezTo>", pt(p[0].0, p[0].1), pt(p[1].0, p[1].1));
                    (cx_, cy_) = p[1];
                }
            }
            'A' => {
                // An arc is drawn as the straight line to its end point.
                for c in nums.chunks(7).filter(|c| c.len() == 7) {
                    let (x, y) = if rel { (cx_ + c[5], cy_ + c[6]) } else { (c[5], c[6]) };
                    let _ = write!(out, "<a:lnTo>{}</a:lnTo>", pt(x, y));
                    (cx_, cy_) = (x, y);
                }
            }
            'Z' => {
                out += "<a:close/>";
                (cx_, cy_) = (sx, sy);
            }
            _ => {}
        }
    }
    out
}

/// The commands of an enhanced path (`draw:enhanced-path`) made of numbers: M L C Z and the ends of subpaths; `None` when it
/// uses a formula or a command that is not drawn.
fn enhanced_path(text: &str) -> Option<String> {
    if text.contains('?') || text.contains('$') {
        return None;
    }
    let mut out = String::new();
    let mut tokens = text.split_whitespace().peekable();
    let mut command = ' ';
    let mut args: Vec<f64> = Vec::new();
    let flush = |command: char, args: &mut Vec<f64>, out: &mut String| -> bool {
        let pt = |x: f64, y: f64| format!(r#"<a:pt x="{}" y="{}"/>"#, x.round() as i64, y.round() as i64);
        match command {
            'M' => {
                for (i, c) in args.chunks(2).filter(|c| c.len() == 2).enumerate() {
                    let tag = if i == 0 { "moveTo" } else { "lnTo" };
                    let _ = write!(out, "<a:{tag}>{}</a:{tag}>", pt(c[0], c[1]));
                }
            }
            'L' => {
                for c in args.chunks(2).filter(|c| c.len() == 2) {
                    let _ = write!(out, "<a:lnTo>{}</a:lnTo>", pt(c[0], c[1]));
                }
            }
            'C' => {
                for c in args.chunks(6).filter(|c| c.len() == 6) {
                    let _ = write!(out, "<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(c[0], c[1]), pt(c[2], c[3]), pt(c[4], c[5]));
                }
            }
            'Z' => out.push_str("<a:close/>"),
            'N' | 'F' | 'S' | ' ' => {}
            _ => return false,
        }
        args.clear();
        true
    };
    while let Some(t) = tokens.next() {
        if t.len() == 1 && t.chars().all(|c| c.is_ascii_alphabetic()) {
            if !flush(command, &mut args, &mut out) {
                return None;
            }
            command = t.chars().next()?;
            if matches!(command, 'Z') && !flush('Z', &mut args, &mut out) {
                return None;
            }
            if matches!(command, 'Z' | 'N' | 'F' | 'S') {
                command = ' ';
            }
        } else {
            args.push(t.parse::<f64>().ok()?);
        }
    }
    flush(command, &mut args, &mut out).then_some(out)
}

/// The shape XML of one drawing object, and nothing for what cannot be shown.
fn object(cx: &mut Cx, n: Node, out: &mut String, fallback: Option<Geom>) {
    if n.tag_name().namespace() != Some(DRAW) {
        return;
    }
    match n.tag_name().name() {
        "frame" => frame(cx, n, out, fallback),
        "g" => group(cx, n, out),
        "rect" | "custom-shape" | "ellipse" | "circle" | "caption" | "polygon" | "polyline" | "path" | "regular-polygon" => shape(cx, n, out),
        "line" | "connector" => line(cx, n, out),
        _ => {}
    }
}

fn next_name(cx: &mut Cx, n: Node, what: &str) -> (u32, String) {
    let id = cx.next_id();
    for key in [attr(n, DRAW, "id"), n.attribute(("http://www.w3.org/XML/1998/namespace", "id"))].into_iter().flatten() {
        cx.shape_ids.insert(key.to_string(), id);
    }
    let name = attr(n, DRAW, "name").map(str::to_string).unwrap_or_else(|| format!("{what} {id}"));
    (id, esc(&name))
}

/// A graphic object with a fill, a line, a shadow and its own text.
fn shape(cx: &mut Cx, n: Node, out: &mut String) {
    let Some(mut g) = (match n.tag_name().name() {
        "polygon" | "polyline" | "path" => geom_of(n),
        _ => geom_of(n),
    }) else { return };
    let chain = style_chain(n);
    let (id, name) = next_name(cx, n, "Shape");
    let name_of = n.tag_name().name();
    let closed = name_of != "polyline" && !(name_of == "path" && false);
    let mut geometry = String::from(r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#);
    let get = |cx: &Cx, ns: &str, local: &str| cx.st.prop_in(&names(&chain), ns, local).map(str::to_string);
    match name_of {
        "ellipse" | "circle" => {
            let kind = attr(n, DRAW, "kind").unwrap_or("full");
            if kind == "full" {
                geometry = r#"<a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>"#.to_string();
            } else {
                let angle = |local: &str| attr(n, DRAW, local).and_then(|a| a.trim_end_matches("deg").parse::<f64>().ok()).unwrap_or(0.0);
                let (start, end) = (angle("start-angle"), angle("end-angle"));
                let clockwise = |a: f64| ((360.0 - a).rem_euclid(360.0) * 60_000.0).round() as i64;
                let (prst, a1, a2) = match kind {
                    "section" => ("pie", clockwise(end), clockwise(start)),
                    "cut" => ("chord", clockwise(end), clockwise(start)),
                    _ => ("arc", clockwise(end), clockwise(start)),
                };
                geometry = format!(r#"<a:prstGeom prst="{prst}">{}</a:prstGeom>"#, avlst(&[("adj1", a1), ("adj2", a2)]));
            }
        }
        "rect" | "caption" => {
            let radius = attr(n, DRAW, "corner-radius").and_then(emu).or_else(|| get(cx, DRAW, "corner-radius").and_then(|r| emu(&r))).unwrap_or(0);
            if radius > 0 {
                let adj = ((radius as f64 / g.w.min(g.h).max(1) as f64) * 100_000.0).round().min(50_000.0) as i64;
                geometry = format!(r#"<a:prstGeom prst="roundRect">{}</a:prstGeom>"#, avlst(&[("adj", adj)]));
            }
        }
        "custom-shape" => {
            if let Some(eg) = children(n, DRAW, "enhanced-geometry").next() {
                let kind = attr(eg, DRAW, "type").unwrap_or("rectangle");
                let preset = preset_of(kind);
                let mut adjust: Vec<(&str, i64)> = Vec::new();
                // Rounded rectangles keep their corner as an adjustment (a fraction of the shorter side, in 1/21600 of the box for the old kinds).
                if let (Some("roundRect"), Some(first)) = (preset.as_deref(), attr(eg, DRAW, "modifiers").and_then(|m| m.split_whitespace().next()).and_then(|v| v.parse::<f64>().ok())) {
                    let adj = if kind.starts_with("ooxml-") { first.round() as i64 } else { (first / 21_600.0 * 100_000.0).round() as i64 };
                    adjust.push(("adj", adj.clamp(0, 50_000)));
                }
                if let Some(p) = preset.filter(|p| !(p == "rect" && kind.ends_with("non-primitive"))) {
                    geometry = format!(r#"<a:prstGeom prst="{p}">{}</a:prstGeom>"#, avlst(&adjust));
                } else if let Some(path) = attr(eg, DRAW, "enhanced-path") {
                    // A path of numbers in the shape's own units (those with formulas are not drawn).
                    let space = attr(eg, "http://openoffice.org/2010/draw", "sub-view-size").map(|v| numbers(v)).filter(|v| v.len() == 2).map(|v| (v[0].round() as i64, v[1].round() as i64)).or_else(|| view_box(eg).map(|(_, _, w, h)| (w, h)));
                    if let (Some((vw, vh)), Some(commands)) = (space.filter(|(w, h)| *w > 0 && *h > 0), enhanced_path(path)) {
                        let fill = get(cx, DRAW, "fill").as_deref() != Some("none");
                        geometry = cust_geom(vw, vh, &commands, fill, true);
                    }
                }
                if attr(eg, DRAW, "mirror-horizontal") == Some("true") {
                    g.flip_h = !g.flip_h;
                }
                if attr(eg, DRAW, "mirror-vertical") == Some("true") {
                    g.flip_v = !g.flip_v;
                }
            }
        }
        "polygon" | "polyline" => {
            if let (Some((vx, vy, vw, vh)), Some(points)) = (view_box(n), attr(n, DRAW, "points").map(point_list)) {
                if points.len() >= 2 {
                    let mut commands = String::new();
                    for (i, (x, y)) in points.iter().enumerate() {
                        let tag = if i == 0 { "moveTo" } else { "lnTo" };
                        let _ = write!(commands, r#"<a:{tag}><a:pt x="{}" y="{}"/></a:{tag}>"#, x - vx, y - vy);
                    }
                    if closed {
                        commands += "<a:close/>";
                    }
                    let fill = get(cx, DRAW, "fill").as_deref() != Some("none");
                    geometry = cust_geom(vw, vh, &commands, fill && closed, true);
                }
            }
        }
        "path" => {
            if let (Some((vx, vy, vw, vh)), Some(d)) = (view_box(n), attr(n, SVG, "d")) {
                let commands = svg_path(d, (vx, vy));
                let fill = get(cx, DRAW, "fill").as_deref() != Some("none");
                geometry = cust_geom(vw, vh, &commands, fill, true);
            }
        }
        _ => {}
    }
    let tchain = text_chain(n);
    cx.shape_rotation = g.rot as f64 / 60_000.0;
    let mut sp_pr = format!("{}{geometry}", xfrm(&g, "a:xfrm"));
    sp_pr += &fill_of(cx, &chain);
    sp_pr += &line_xml(cx, &chain);
    sp_pr += &shadow_xml(cx, &chain);
    let tx = if has_text(n) { format!("<p:txBody>{}{}</p:txBody>", body_pr(cx, &chain), format_args!("<a:lstStyle/>{}", paragraphs(cx, n, &tchain, Body::default()))) } else { String::new() };
    let tx = if tx.is_empty() { format!(r#"<p:txBody>{}<a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody>"#, body_pr(cx, &chain)) } else { tx };
    let _ = write!(out, r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{sp_pr}</p:spPr>{tx}</p:sp>"#);
}

/// A straight line or connector.
fn line(cx: &mut Cx, n: Node, out: &mut String) {
    let len = |local: &str| attr(n, SVG, local).and_then(emu);
    let (Some(x1), Some(y1), Some(x2), Some(y2)) = (len("x1"), len("y1"), len("x2"), len("y2")) else { return };
    let g = Geom { x: x1.min(x2), y: y1.min(y2), w: (x2 - x1).abs(), h: (y2 - y1).abs(), flip_h: x2 < x1, flip_v: y2 < y1, ..Default::default() };
    let chain = style_chain(n);
    let (id, name) = next_name(cx, n, "Line");
    let mut sp_pr = format!(r#"{}<a:prstGeom prst="line"><a:avLst/></a:prstGeom>"#, xfrm(&g, "a:xfrm"));
    let ln = line_xml(cx, &chain);
    sp_pr += &if ln.is_empty() { "<a:ln><a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill></a:ln>".to_string() } else { ln };
    sp_pr += &shadow_xml(cx, &chain);
    let _ = write!(out, r#"<p:cxnSp><p:nvCxnSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr><p:spPr>{sp_pr}</p:spPr></p:cxnSp>"#);
}

fn group(cx: &mut Cx, n: Node, out: &mut String) {
    let (id, name) = next_name(cx, n, "Group");
    let mut inner = String::new();
    let mut bounds: Option<(i64, i64, i64, i64)> = None;
    for c in n.children().filter(|c| c.is_element()) {
        if let Some(g) = child_bounds(c) {
            bounds = Some(match bounds {
                None => (g.x, g.y, g.x + g.w, g.y + g.h),
                Some((a, b, c2, d)) => (a.min(g.x), b.min(g.y), c2.max(g.x + g.w), d.max(g.y + g.h)),
            });
        }
        object(cx, c, &mut inner, None);
    }
    let Some((x0, y0, x1, y1)) = bounds else { return };
    let (w, h) = (x1 - x0, y1 - y0);
    let _ = write!(
        out,
        r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="{x0}" y="{y0}"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="{x0}" y="{y0}"/><a:chExt cx="{w}" cy="{h}"/></a:xfrm></p:grpSpPr>{inner}</p:grpSp>"#
    );
}

/// The box of a drawing object (a line's two ends, or its position and size).
pub(crate) fn child_bounds(n: Node) -> Option<Geom> {
    if n.tag_name().namespace() != Some(DRAW) {
        return None;
    }
    match n.tag_name().name() {
        "line" | "connector" => {
            let len = |local: &str| attr(n, SVG, local).and_then(emu);
            let (x1, y1, x2, y2) = (len("x1")?, len("y1")?, len("x2")?, len("y2")?);
            Some(Geom { x: x1.min(x2), y: y1.min(y2), w: (x2 - x1).abs(), h: (y2 - y1).abs(), ..Default::default() })
        }
        "g" => {
            let mut b: Option<(i64, i64, i64, i64)> = None;
            for c in n.children().filter(|c| c.is_element()) {
                if let Some(g) = child_bounds(c) {
                    b = Some(match b {
                        None => (g.x, g.y, g.x + g.w, g.y + g.h),
                        Some((a, bb, c2, d)) => (a.min(g.x), bb.min(g.y), c2.max(g.x + g.w), d.max(g.y + g.h)),
                    });
                }
            }
            b.map(|(x0, y0, x1, y1)| Geom { x: x0, y: y0, w: x1 - x0, h: y1 - y0, ..Default::default() })
        }
        _ => geom_of(n),
    }
}

/// A picture's crop (`fo:clip`: "rect(top, right, bottom, left)" of the picture at its own size) as `a:srcRect`.
fn src_rect(cx: &Cx, chain: &[(&'static str, String)], size: Option<(f64, f64)>) -> String {
    let Some(clip) = cx.st.prop_in(&names(chain), FO, "clip") else { return String::new() };
    let Some(inner) = clip.trim().strip_prefix("rect(").and_then(|c| c.strip_suffix(')')) else { return String::new() };
    let sides: Vec<i64> = inner.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).filter_map(emu).collect();
    let (Some((iw, ih)), 4) = (size, sides.len()) else { return String::new() };
    let frac = |v: i64, total: f64| ((v as f64 / total) * 100_000.0).round().clamp(0.0, 99_000.0) as i64;
    let (t, r, b, l) = (frac(sides[0], ih), frac(sides[1], iw), frac(sides[2], ih), frac(sides[3], iw));
    if t == 0 && r == 0 && b == 0 && l == 0 {
        return String::new();
    }
    format!(r#"<a:srcRect l="{l}" t="{t}" r="{r}" b="{b}"/>"#)
}

fn frame(cx: &mut Cx, n: Node, out: &mut String, fallback: Option<Geom>) {
    let class = attr(n, PRESENTATION, "class").unwrap_or("");
    let g = match geom_of(n) {
        Some(g) => Some(g),
        None => fallback,
    };
    let Some(g) = g else { return };
    let chain = style_chain(n);
    for c in n.children().filter(|c| c.is_element()) {
        if c.tag_name().namespace() == Some(DRAW) {
            match c.tag_name().name() {
                "text-box" => {
                    text_frame(cx, n, c, g, class, &chain, out);
                    return;
                }
                "image" => {
                    picture(cx, n, c, g, &chain, out);
                    return;
                }
                "object" | "object-ole" => {
                    // A chart or embedded document: the picture it was last drawn as.
                    if let Some(img) = n.children().find(|x| is(*x, DRAW, "image")) {
                        picture(cx, n, img, g, &chain, out);
                    }
                    return;
                }
                _ => {}
            }
        } else if is(c, TABLE, "table") {
            table(cx, n, c, g, out);
            return;
        }
    }
}

fn text_frame(cx: &mut Cx, frame: Node, text_box: Node, g: Geom, class: &str, chain: &[(&'static str, String)], out: &mut String) {
    let (id, name) = next_name(cx, frame, "Text");
    let tchain = text_chain(frame);
    let (ph, body) = match class {
        "title" => (r#"<p:ph type="title"/>"#.to_string(), Body::default()),
        "subtitle" => (r#"<p:ph type="subTitle" idx="1"/>"#.to_string(), Body::default()),
        "outline" => (r#"<p:ph idx="1"/>"#.to_string(), Body { placeholder_body: true }),
        _ => (String::new(), Body::default()),
    };
    let c_nv_sp_pr = if ph.is_empty() { r#"<p:cNvSpPr txBox="1"/>"# } else { r#"<p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>"# };
    cx.shape_rotation = g.rot as f64 / 60_000.0;
    let mut sp_pr = format!(r#"{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#, xfrm(&g, "a:xfrm"));
    sp_pr += &fill_of(cx, chain);
    sp_pr += &line_xml(cx, chain);
    sp_pr += &shadow_xml(cx, chain);
    let paragraphs = paragraphs(cx, text_box, &tchain, body);
    let body_pr = body_pr(cx, chain);
    let _ = write!(out, r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/>{c_nv_sp_pr}<p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr>{sp_pr}</p:spPr><p:txBody>{body_pr}<a:lstStyle/>{paragraphs}</p:txBody></p:sp>"#);
}

/// The pixel size of a picture file (PNG and JPEG), and the density if it says one, as the picture's size in points.
fn picture_size(data: &[u8]) -> Option<(f64, f64)> {
    if data.starts_with(b"\x89PNG") {
        let w = u32::from_be_bytes(data.get(16..20)?.try_into().ok()?) as f64;
        let h = u32::from_be_bytes(data.get(20..24)?.try_into().ok()?) as f64;
        // The physical size, when given (pHYs: pixels per metre).
        let mut at = 8;
        while at + 12 <= data.len() {
            let len = u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize;
            if data.get(at + 4..at + 8)? == b"pHYs" {
                let ppx = u32::from_be_bytes(data.get(at + 8..at + 12)?.try_into().ok()?) as f64;
                let ppy = u32::from_be_bytes(data.get(at + 12..at + 16)?.try_into().ok()?) as f64;
                let unit = *data.get(at + 16)?;
                if unit == 1 && ppx > 0.0 && ppy > 0.0 {
                    return Some((w / ppx * 72.0 / 0.0254, h / ppy * 72.0 / 0.0254));
                }
            }
            at += 12 + len;
        }
        return Some((w * 0.75, h * 0.75));
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        let mut at = 2;
        let mut density: Option<(f64, f64)> = None;
        while at + 9 < data.len() {
            if data[at] != 0xFF {
                at += 1;
                continue;
            }
            let marker = data[at + 1];
            let len = usize::from(u16::from_be_bytes([data[at + 2], data[at + 3]]));
            if marker == 0xE0 && data.get(at + 4..at + 9) == Some(b"JFIF\0") {
                let unit = *data.get(at + 11)?;
                let dx = f64::from(u16::from_be_bytes([*data.get(at + 12)?, *data.get(at + 13)?]));
                let dy = f64::from(u16::from_be_bytes([*data.get(at + 14)?, *data.get(at + 15)?]));
                density = match unit {
                    1 if dx > 0.0 && dy > 0.0 => Some((72.0 / dx, 72.0 / dy)),
                    2 if dx > 0.0 && dy > 0.0 => Some((72.0 / 2.54 / dx, 72.0 / 2.54 / dy)),
                    _ => None,
                };
            }
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let h = f64::from(u16::from_be_bytes([data[at + 5], data[at + 6]]));
                let w = f64::from(u16::from_be_bytes([data[at + 7], data[at + 8]]));
                let (px, py) = density.unwrap_or((0.75, 0.75));
                return Some((w * px, h * py));
            }
            at += 2 + len;
        }
    }
    None
}

fn picture(cx: &mut Cx, frame: Node, image: Node, g: Geom, chain: &[(&'static str, String)], out: &mut String) {
    let Some(href) = attr(image, XLINK, "href") else { return };
    let Some((rid, size)) = cx.image_rel_sized(href, picture_size) else { return };
    let (id, name) = next_name(cx, frame, "Picture");
    let crop = src_rect(cx, chain, size.map(|(w, h)| (w * 12_700.0, h * 12_700.0)));
    let mut sp_pr = format!(r#"{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#, xfrm(&g, "a:xfrm"));
    sp_pr += &line_xml(cx, chain);
    sp_pr += &shadow_xml(cx, chain);
    let descr = children(frame, SVG, "desc").next().and_then(|d| d.text()).map(esc).unwrap_or_default();
    let descr = if descr.is_empty() { String::new() } else { format!(r#" descr="{descr}""#) };
    let _ = write!(out, r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{name}"{descr}/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rid}"/>{crop}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{sp_pr}</p:spPr></p:pic>"#);
}

/// A table's cell border from "0.05pt solid #000000".
fn border(text: Option<&str>, tag: &str) -> String {
    let Some(t) = text else { return String::new() };
    let t = t.trim();
    if t == "none" || t.starts_with("0cm") && t.contains("none") {
        return format!("<a:{tag}><a:noFill/></a:{tag}>");
    }
    let parts: Vec<&str> = t.split_whitespace().collect();
    let width = parts.first().and_then(|w| emu(w)).unwrap_or(12_700).max(1);
    let c = parts.iter().find_map(|p| color(p)).unwrap_or_else(|| "000000".to_string());
    if parts.iter().any(|p| *p == "none" || *p == "hidden") {
        return format!("<a:{tag}><a:noFill/></a:{tag}>");
    }
    let dash = if parts.iter().any(|p| *p == "dashed") { r#"<a:prstDash val="dash"/>"# } else if parts.iter().any(|p| *p == "dotted") { r#"<a:prstDash val="sysDot"/>"# } else { "" };
    format!(r#"<a:{tag} w="{width}"><a:solidFill><a:srgbClr val="{c}"/></a:solidFill>{dash}</a:{tag}>"#)
}

fn table(cx: &mut Cx, frame: Node, table: Node, g: Geom, out: &mut String) {
    // Columns (with repeats) and rows, through the header and body groups.
    let mut columns: Vec<i64> = Vec::new();
    let mut rows: Vec<Node> = Vec::new();
    fn collect<'a, 'i>(cx: &Cx, node: Node<'a, 'i>, columns: &mut Vec<i64>, rows: &mut Vec<Node<'a, 'i>>) {
        for c in node.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(TABLE)) {
            match c.tag_name().name() {
                "table-column" => {
                    let repeat = attr(c, TABLE, "number-columns-repeated").and_then(|r| r.parse::<usize>().ok()).unwrap_or(1).min(256);
                    let width = attr(c, TABLE, "style-name").and_then(|s| cx.st.prop("table-column", s, STYLE, "column-width")).and_then(emu).unwrap_or(914_400);
                    columns.extend(std::iter::repeat_n(width, repeat));
                }
                "table-row" => rows.push(c),
                "table-columns" | "table-header-columns" | "table-header-rows" | "table-rows" | "table-row-group" => collect(cx, c, columns, rows),
                _ => {}
            }
        }
    }
    collect(cx, table, &mut columns, &mut rows);
    if columns.is_empty() || rows.is_empty() {
        return;
    }
    let (id, name) = next_name(cx, frame, "Table");
    let mut grid = String::new();
    for w in &columns {
        let _ = write!(grid, r#"<a:gridCol w="{w}"/>"#);
    }
    let mut body = String::new();
    let mut total_h = 0;
    // The rows covered by a cell spanning down, per column.
    let mut covered_down = vec![0usize; columns.len()];
    for row in &rows {
        let height = attr(*row, TABLE, "style-name").and_then(|s| cx.st.prop("table-row", s, STYLE, "row-height").or_else(|| cx.st.prop("table-row", s, STYLE, "min-row-height"))).and_then(emu).unwrap_or(370_840);
        total_h += height;
        let mut cells = String::new();
        let mut col = 0;
        for cell in row.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(TABLE)) {
            let covered = cell.tag_name().name() == "covered-table-cell";
            if !covered && cell.tag_name().name() != "table-cell" {
                continue;
            }
            let repeat = attr(cell, TABLE, "number-columns-repeated").and_then(|r| r.parse::<usize>().ok()).unwrap_or(1).min(columns.len().saturating_sub(col).max(1));
            for _ in 0..repeat {
                if col >= columns.len() {
                    break;
                }
                let colspan = attr(cell, TABLE, "number-columns-spanned").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
                let rowspan = attr(cell, TABLE, "number-rows-spanned").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
                let style = attr(cell, TABLE, "style-name").map(str::to_string);
                let mut attrs = String::new();
                if !covered {
                    if colspan > 1 {
                        let _ = write!(attrs, r#" gridSpan="{colspan}""#);
                    }
                    if rowspan > 1 {
                        let _ = write!(attrs, r#" rowSpan="{rowspan}""#);
                        covered_down[col] = rowspan - 1;
                    }
                } else {
                    let below = covered_down[col] > 0;
                    if below {
                        covered_down[col] -= 1;
                        attrs += r#" vMerge="1""#;
                    }
                    // (A cell covered from the left of its own row.)
                    if !below || colspan > 1 {
                        attrs += r#" hMerge="1""#;
                    }
                }
                let chain: Vec<(&'static str, String)> = style.iter().map(|s| ("table-cell", s.clone())).collect();
                let get = |cx: &Cx, local: &str| cx.st.prop_in(&names(&chain), FO, local).map(str::to_string);
                let mut tc_pr_attrs = String::new();
                for (side, name) in [("left", "marL"), ("top", "marT"), ("right", "marR"), ("bottom", "marB")] {
                    if let Some(v) = get(cx, &format!("padding-{side}")).or_else(|| get(cx, "padding")).and_then(|v| emu(&v)) {
                        let _ = write!(tc_pr_attrs, r#" {name}="{v}""#);
                    }
                }
                match cx.st.prop_in(&names(&chain), STYLE, "vertical-align") {
                    Some("middle") => tc_pr_attrs += r#" anchor="ctr""#,
                    Some("bottom") => tc_pr_attrs += r#" anchor="b""#,
                    _ => {}
                }
                let all = get(cx, "border");
                let mut inner = String::new();
                for (side, tag) in [("left", "lnL"), ("right", "lnR"), ("top", "lnT"), ("bottom", "lnB")] {
                    let b = get(cx, &format!("border-{side}")).or_else(|| all.clone());
                    inner += &border(b.as_deref(), tag);
                }
                if let Some(c) = get(cx, "background-color").and_then(|c| color(&c)) {
                    let _ = write!(inner, r#"<a:solidFill><a:srgbClr val="{c}"/></a:solidFill>"#);
                } else if get(cx, "background-color").as_deref() == Some("transparent") {
                    inner += "<a:noFill/>";
                }
                let mut text_chain: Vec<(&'static str, String)> = vec![];
                if let Some(s) = &style {
                    text_chain.push(("table-cell", s.clone()));
                }
                let paras = if covered { "<a:p><a:endParaRPr lang=\"en-US\"/></a:p>".to_string() } else { paragraphs(cx, cell, &text_chain, Body::default()) };
                let _ = write!(cells, r#"<a:tc{attrs}><a:txBody><a:bodyPr/><a:lstStyle/>{paras}</a:txBody><a:tcPr{tc_pr_attrs}>{inner}</a:tcPr></a:tc>"#);
                col += colspan.max(1).min(1) ;
            }
        }
        let _ = write!(body, r#"<a:tr h="{height}">{cells}</a:tr>"#);
    }
    let width: i64 = columns.iter().sum();
    let g = Geom { w: width, h: total_h.max(g.h), ..g };
    let _ = write!(
        out,
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="{name}"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr/><a:tblGrid>{grid}</a:tblGrid>{body}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        xfrm(&Geom { rot: 0, flip_h: false, flip_v: false, ..g }, "p:xfrm")
    );
}

/// The objects of a page or master page, in order, as shape XML. `placeholders` gives the box of a placeholder class when the
/// object does not carry its own.
pub(crate) fn objects(cx: &mut Cx, parent: Node, skip_placeholders: bool, placeholder_box: &dyn Fn(&str) -> Option<Geom>) -> String {
    let mut out = String::new();
    for n in parent.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(DRAW)) {
        let class = attr(n, PRESENTATION, "class").unwrap_or("");
        if skip_placeholders && n.tag_name().name() == "frame" && matches!(class, "title" | "subtitle" | "outline" | "notes" | "date-time" | "footer" | "header" | "page-number" | "page" | "handout") {
            continue;
        }
        if n.tag_name().name() == "page-thumbnail" {
            continue;
        }
        let fallback = if class.is_empty() { None } else { placeholder_box(class) };
        object(cx, n, &mut out, fallback);
    }
    out
}
