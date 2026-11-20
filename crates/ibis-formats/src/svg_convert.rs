// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Converts SVG vector graphics into Office DrawingML custom shapes (`<a:custGeom>`).
//!
//! PowerPoint's "Convert to Shape" feature takes an inserted vector SVG and
//! converts its elements into native Office shapes with fills, strokes, and
//! freeform paths (`Geometry::Formula(FormulaGeometry { preset: None, ... })`).

use std::sync::Arc;

use ibis_doc_model::{
    AutoShape, Color, EmuRect, Fill, FormulaGeometry, Geometry, GeometryDef, GeometryPath, PathFill, PathOp,
    Rgb, Shape, ShapeId, ShapeKind, Stroke,
};
use roxmltree::Node;

use crate::Error;

/// Converts an SVG document into one or more DrawingML shapes covering `frame`.
pub fn convert_svg_to_shapes(
    mut new_shape_id: impl FnMut() -> ShapeId,
    svg_data: &[u8],
    frame: EmuRect,
) -> Result<Vec<Shape>, Error> {
    let text = std::str::from_utf8(svg_data).map_err(|e| Error::new(format!("SVG の文字コードが不正です: {e}")))?;
    let doc = roxmltree::Document::parse(text).map_err(|e| Error::new(format!("SVG を解析できません: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(Error::new("ルート要素が svg ではありません"));
    }

    // Determine viewBox / dimension
    let view_box = parse_view_box(&root);
    let (vx, vy, vw, vh) = view_box.unwrap_or_else(|| {
        let w = parse_length(root.attribute("width")).unwrap_or(100.0);
        let h = parse_length(root.attribute("height")).unwrap_or(100.0);
        (0.0, 0.0, w.max(1.0), h.max(1.0))
    });

    let mut collector = SvgCollector {
        vx,
        vy,
        vw: vw.max(1.0),
        vh: vh.max(1.0),
        elements: Vec::new(),
    };

    let default_style = SvgStyle::default();
    collector.collect_node(&root, &default_style);

    if collector.elements.is_empty() {
        return Err(Error::new("SVG に図形要素が見つかりません"));
    }

    let mut shapes = Vec::new();
    let norm_w = (collector.vw * 1000.0).round() as i64;
    let norm_h = (collector.vh * 1000.0).round() as i64;

    for item in collector.elements {
        let path = GeometryPath {
            width: norm_w,
            height: norm_h,
            fill: if item.fill.is_none() { PathFill::None } else { PathFill::Normal },
            stroke: item.stroke.is_some(),
            ops: item.ops,
        };

        let def = GeometryDef {
            adjust: Vec::new(),
            guides: Vec::new(),
            paths: vec![path],
            text_rect: None,
        };

        let geometry = Geometry::Formula(FormulaGeometry {
            preset: None,
            adjust: Vec::new(),
            def: Arc::new(def),
        });

        let auto = AutoShape {
            geometry,
            fill: item.fill,
            stroke: item.stroke,
            text: None,
            tilt: None,
            shadow: None,
            glow: None,
            inner_shadow: None,
            soft_edge: None,
            reflection: None,
            bevel: None,
        };

        let shape = Shape {
            id: new_shape_id(),
            frame,
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ShapeKind::Auto(auto),
            floating: None,
        };

        shapes.push(shape);
    }

    Ok(shapes)
}

#[derive(Clone, Debug)]
struct SvgStyle {
    fill: Option<Fill>,
    stroke: Option<Stroke>,
    opacity: f64,
}

impl Default for SvgStyle {
    fn default() -> Self {
        Self {
            fill: Some(Fill::Solid(Color::BLACK)),
            stroke: None,
            opacity: 1.0,
        }
    }
}

struct CollectedElement {
    ops: Vec<PathOp>,
    fill: Fill,
    stroke: Option<Stroke>,
}

struct SvgCollector {
    vx: f64,
    vy: f64,
    vw: f64,
    vh: f64,
    elements: Vec<CollectedElement>,
}

impl SvgCollector {
    fn collect_node(&mut self, node: &Node, parent_style: &SvgStyle) {
        let mut style = parent_style.clone();

        // Check opacity
        if let Some(op_str) = node.attribute("opacity") {
            if let Ok(v) = op_str.trim().parse::<f64>() {
                style.opacity = (style.opacity * v.clamp(0.0, 1.0)).clamp(0.0, 1.0);
            }
        }

        // Check fill
        if let Some(fill_attr) = node.attribute("fill") {
            let fill_op = node.attribute("fill-opacity").and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(1.0);
            let total_op = style.opacity * fill_op.clamp(0.0, 1.0);
            style.fill = parse_fill(fill_attr, total_op);
        }

        // Check stroke
        if let Some(stroke_attr) = node.attribute("stroke") {
            let stroke_op = node.attribute("stroke-opacity").and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(1.0);
            let total_op = style.opacity * stroke_op.clamp(0.0, 1.0);
            let width = parse_length(node.attribute("stroke-width")).unwrap_or(1.0);
            // Default 1 px ~ 9525 EMU (0.75 pt)
            let width_emu = (width * 9_525.0).round().max(1.0) as i64;
            style.stroke = parse_stroke(stroke_attr, total_op, width_emu);
        }

        match node.tag_name().name() {
            "path" => {
                if let Some(d) = node.attribute("d") {
                    let mut ops = Vec::new();
                    parse_svg_path(d, self.vx, self.vy, &mut ops);
                    if !ops.is_empty() {
                        self.elements.push(CollectedElement {
                            ops,
                            fill: style.fill.clone().unwrap_or(Fill::None),
                            stroke: style.stroke.clone(),
                        });
                    }
                }
            }
            "rect" => {
                let x = parse_length(node.attribute("x")).unwrap_or(0.0);
                let y = parse_length(node.attribute("y")).unwrap_or(0.0);
                if let (Some(w), Some(h)) = (parse_length(node.attribute("width")), parse_length(node.attribute("height"))) {
                    if w > 0.0 && h > 0.0 {
                        let rx = parse_length(node.attribute("rx"));
                        let ry = parse_length(node.attribute("ry"));
                        let mut ops = Vec::new();
                        build_rect_path(x, y, w, h, rx, ry, self.vx, self.vy, &mut ops);
                        self.elements.push(CollectedElement {
                            ops,
                            fill: style.fill.clone().unwrap_or(Fill::None),
                            stroke: style.stroke.clone(),
                        });
                    }
                }
            }
            "circle" => {
                let cx = parse_length(node.attribute("cx")).unwrap_or(0.0);
                let cy = parse_length(node.attribute("cy")).unwrap_or(0.0);
                if let Some(r) = parse_length(node.attribute("r")) {
                    if r > 0.0 {
                        let mut ops = Vec::new();
                        build_ellipse_path(cx, cy, r, r, self.vx, self.vy, &mut ops);
                        self.elements.push(CollectedElement {
                            ops,
                            fill: style.fill.clone().unwrap_or(Fill::None),
                            stroke: style.stroke.clone(),
                        });
                    }
                }
            }
            "ellipse" => {
                let cx = parse_length(node.attribute("cx")).unwrap_or(0.0);
                let cy = parse_length(node.attribute("cy")).unwrap_or(0.0);
                if let (Some(rx), Some(ry)) = (parse_length(node.attribute("rx")), parse_length(node.attribute("ry"))) {
                    if rx > 0.0 && ry > 0.0 {
                        let mut ops = Vec::new();
                        build_ellipse_path(cx, cy, rx, ry, self.vx, self.vy, &mut ops);
                        self.elements.push(CollectedElement {
                            ops,
                            fill: style.fill.clone().unwrap_or(Fill::None),
                            stroke: style.stroke.clone(),
                        });
                    }
                }
            }
            "line" => {
                let x1 = parse_length(node.attribute("x1")).unwrap_or(0.0);
                let y1 = parse_length(node.attribute("y1")).unwrap_or(0.0);
                let x2 = parse_length(node.attribute("x2")).unwrap_or(0.0);
                let y2 = parse_length(node.attribute("y2")).unwrap_or(0.0);
                let ops = vec![
                    PathOp::MoveTo(to_norm(x1, self.vx), to_norm(y1, self.vy)),
                    PathOp::LineTo(to_norm(x2, self.vx), to_norm(y2, self.vy)),
                ];
                self.elements.push(CollectedElement {
                    ops,
                    fill: Fill::None,
                    stroke: style.stroke.clone().or_else(|| Some(Stroke::solid(Color::BLACK, 9_525))),
                });
            }
            "polygon" => {
                if let Some(pts) = node.attribute("points") {
                    let mut ops = Vec::new();
                    build_points_path(pts, true, self.vx, self.vy, &mut ops);
                    if !ops.is_empty() {
                        self.elements.push(CollectedElement {
                            ops,
                            fill: style.fill.clone().unwrap_or(Fill::None),
                            stroke: style.stroke.clone(),
                        });
                    }
                }
            }
            "polyline" => {
                if let Some(pts) = node.attribute("points") {
                    let mut ops = Vec::new();
                    build_points_path(pts, false, self.vx, self.vy, &mut ops);
                    if !ops.is_empty() {
                        self.elements.push(CollectedElement {
                            ops,
                            fill: style.fill.clone().unwrap_or(Fill::None),
                            stroke: style.stroke.clone(),
                        });
                    }
                }
            }
            _ => {}
        }

        for child in node.children().filter(|n| n.is_element()) {
            self.collect_node(&child, &style);
        }
    }
}

fn to_norm(val: f64, origin: f64) -> String {
    let scaled = (val - origin) * 1000.0;
    scaled.round().to_string()
}

fn parse_view_box(node: &Node) -> Option<(f64, f64, f64, f64)> {
    let vb = node.attribute("viewBox")?;
    let mut parts = vb.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty());
    let x = parts.next()?.parse::<f64>().ok()?;
    let y = parts.next()?.parse::<f64>().ok()?;
    let w = parts.next()?.parse::<f64>().ok()?;
    let h = parts.next()?.parse::<f64>().ok()?;
    Some((x, y, w, h))
}

fn parse_length(s: Option<&str>) -> Option<f64> {
    let s = s?.trim();
    if s.is_empty() {
        return None;
    }
    let s = s.trim_end_matches("px").trim_end_matches("pt");
    s.parse::<f64>().ok()
}

fn parse_fill(s: &str, opacity: f64) -> Option<Fill> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(Fill::None);
    }
    let rgb = parse_color_rgb(s)?;
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Fill::Solid(Color { rgb, alpha }))
}

fn parse_stroke(s: &str, opacity: f64, width: i64) -> Option<Stroke> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return None;
    }
    let rgb = parse_color_rgb(s)?;
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Stroke::solid(Color { rgb, alpha }, width))
}

fn parse_color_rgb(s: &str) -> Option<Rgb> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let val = u32::from_str_radix(hex, 16).ok()?;
            return Some(Rgb(val));
        } else if hex.len() == 3 {
            // `get`: a multi-byte character in the value is not a colour (slicing inside it panics).
            let r = u8::from_str_radix(hex.get(0..1)?, 16).ok()?;
            let g = u8::from_str_radix(hex.get(1..2)?, 16).ok()?;
            let b = u8::from_str_radix(hex.get(2..3)?, 16).ok()?;
            return Some(Rgb::from_rgb((r << 4) | r, (g << 4) | g, (b << 4) | b));
        }
    }
    if s.starts_with("rgb(") && s.ends_with(')') {
        let inside = &s[4..s.len() - 1];
        let mut parts = inside.split(',').map(str::trim);
        let r = parts.next()?.parse::<u8>().ok()?;
        let g = parts.next()?.parse::<u8>().ok()?;
        let b = parts.next()?.parse::<u8>().ok()?;
        return Some(Rgb::from_rgb(r, g, b));
    }
    match s.to_ascii_lowercase().as_str() {
        "black" => Some(Rgb::BLACK),
        "white" => Some(Rgb::WHITE),
        "red" => Some(Rgb(0xff0000)),
        "green" => Some(Rgb(0x008000)),
        "lime" => Some(Rgb(0x00ff00)),
        "blue" => Some(Rgb(0x0000ff)),
        "yellow" => Some(Rgb(0xffff00)),
        "cyan" | "aqua" => Some(Rgb(0x00ffff)),
        "magenta" | "fuchsia" => Some(Rgb(0xff00ff)),
        "gray" | "grey" => Some(Rgb(0x808080)),
        "silver" => Some(Rgb(0xc0c0c0)),
        "maroon" => Some(Rgb(0x800000)),
        "navy" => Some(Rgb(0x000080)),
        "purple" => Some(Rgb(0x800080)),
        "teal" => Some(Rgb(0x008080)),
        "orange" => Some(Rgb(0xffa500)),
        _ => None,
    }
}

fn build_rect_path(x: f64, y: f64, w: f64, h: f64, rx: Option<f64>, ry: Option<f64>, vx: f64, vy: f64, ops: &mut Vec<PathOp>) {
    let rad_x = rx.unwrap_or_else(|| ry.unwrap_or(0.0)).min(w / 2.0);
    let rad_y = ry.unwrap_or_else(|| rx.unwrap_or(0.0)).min(h / 2.0);

    if rad_x <= 0.0 || rad_y <= 0.0 {
        ops.push(PathOp::MoveTo(to_norm(x, vx), to_norm(y, vy)));
        ops.push(PathOp::LineTo(to_norm(x + w, vx), to_norm(y, vy)));
        ops.push(PathOp::LineTo(to_norm(x + w, vx), to_norm(y + h, vy)));
        ops.push(PathOp::LineTo(to_norm(x, vx), to_norm(y + h, vy)));
        ops.push(PathOp::Close);
    } else {
        let kx = rad_x * 0.5522847498307935;
        let ky = rad_y * 0.5522847498307935;
        ops.push(PathOp::MoveTo(to_norm(x + rad_x, vx), to_norm(y, vy)));
        ops.push(PathOp::LineTo(to_norm(x + w - rad_x, vx), to_norm(y, vy)));
        ops.push(PathOp::CubicTo([
            to_norm(x + w - rad_x + kx, vx), to_norm(y, vy),
            to_norm(x + w, vx), to_norm(y + rad_y - ky, vy),
            to_norm(x + w, vx), to_norm(y + rad_y, vy),
        ]));
        ops.push(PathOp::LineTo(to_norm(x + w, vx), to_norm(y + h - rad_y, vy)));
        ops.push(PathOp::CubicTo([
            to_norm(x + w, vx), to_norm(y + h - rad_y + ky, vy),
            to_norm(x + w - rad_x + kx, vx), to_norm(y + h, vy),
            to_norm(x + w - rad_x, vx), to_norm(y + h, vy),
        ]));
        ops.push(PathOp::LineTo(to_norm(x + rad_x, vx), to_norm(y + h, vy)));
        ops.push(PathOp::CubicTo([
            to_norm(x + rad_x - kx, vx), to_norm(y + h, vy),
            to_norm(x, vx), to_norm(y + h - rad_y + ky, vy),
            to_norm(x, vx), to_norm(y + h - rad_y, vy),
        ]));
        ops.push(PathOp::LineTo(to_norm(x, vx), to_norm(y + rad_y, vy)));
        ops.push(PathOp::CubicTo([
            to_norm(x, vx), to_norm(y + rad_y - ky, vy),
            to_norm(x + rad_x - kx, vx), to_norm(y, vy),
            to_norm(x + rad_x, vx), to_norm(y, vy),
        ]));
        ops.push(PathOp::Close);
    }
}

fn build_ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64, vx: f64, vy: f64, ops: &mut Vec<PathOp>) {
    let kx = rx * 0.5522847498307935;
    let ky = ry * 0.5522847498307935;
    ops.push(PathOp::MoveTo(to_norm(cx, vx), to_norm(cy - ry, vy)));
    ops.push(PathOp::CubicTo([
        to_norm(cx + kx, vx), to_norm(cy - ry, vy),
        to_norm(cx + rx, vx), to_norm(cy - ky, vy),
        to_norm(cx + rx, vx), to_norm(cy, vy),
    ]));
    ops.push(PathOp::CubicTo([
        to_norm(cx + rx, vx), to_norm(cy + ky, vy),
        to_norm(cx + kx, vx), to_norm(cy + ry, vy),
        to_norm(cx, vx), to_norm(cy + ry, vy),
    ]));
    ops.push(PathOp::CubicTo([
        to_norm(cx - kx, vx), to_norm(cy + ry, vy),
        to_norm(cx - rx, vx), to_norm(cy + ky, vy),
        to_norm(cx - rx, vx), to_norm(cy, vy),
    ]));
    ops.push(PathOp::CubicTo([
        to_norm(cx - rx, vx), to_norm(cy - ky, vy),
        to_norm(cx - kx, vx), to_norm(cy - ry, vy),
        to_norm(cx, vx), to_norm(cy - ry, vy),
    ]));
    ops.push(PathOp::Close);
}

fn build_points_path(pts_str: &str, closed: bool, vx: f64, vy: f64, ops: &mut Vec<PathOp>) {
    let mut reader = SvgPathReader::new(pts_str);
    let mut first = true;
    while let (Some(x), Some(y)) = (reader.next_number(), reader.next_number()) {
        if first {
            ops.push(PathOp::MoveTo(to_norm(x, vx), to_norm(y, vy)));
            first = false;
        } else {
            ops.push(PathOp::LineTo(to_norm(x, vx), to_norm(y, vy)));
        }
    }
    if closed && !first {
        ops.push(PathOp::Close);
    }
}

struct SvgPathReader<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> SvgPathReader<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn skip_whitespace_and_commas(&mut self) {
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() && (bytes[self.pos].is_ascii_whitespace() || bytes[self.pos] == b',') {
            self.pos += 1;
        }
    }

    fn next_command(&mut self) -> Option<char> {
        self.skip_whitespace_and_commas();
        let bytes = self.src.as_bytes();
        if self.pos < bytes.len() && bytes[self.pos].is_ascii_alphabetic() {
            let c = bytes[self.pos] as char;
            self.pos += 1;
            Some(c)
        } else {
            None
        }
    }

    fn next_number(&mut self) -> Option<f64> {
        self.skip_whitespace_and_commas();
        let bytes = self.src.as_bytes();
        let start = self.pos;
        if self.pos >= bytes.len() {
            return None;
        }
        if bytes[self.pos] == b'+' || bytes[self.pos] == b'-' {
            self.pos += 1;
        }
        let mut has_digit = false;
        while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
            has_digit = true;
            self.pos += 1;
        }
        if self.pos < bytes.len() && bytes[self.pos] == b'.' {
            self.pos += 1;
            while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
                has_digit = true;
                self.pos += 1;
            }
        }
        if has_digit && self.pos < bytes.len() && (bytes[self.pos] == b'e' || bytes[self.pos] == b'E') {
            let e_pos = self.pos;
            self.pos += 1;
            if self.pos < bytes.len() && (bytes[self.pos] == b'+' || bytes[self.pos] == b'-') {
                self.pos += 1;
            }
            let mut has_exp_digit = false;
            while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
                has_exp_digit = true;
                self.pos += 1;
            }
            if !has_exp_digit {
                self.pos = e_pos;
            }
        }
        if has_digit {
            self.src[start..self.pos].parse::<f64>().ok()
        } else {
            self.pos = start;
            None
        }
    }
}

fn parse_svg_path(d: &str, vx: f64, vy: f64, ops: &mut Vec<PathOp>) {
    let mut reader = SvgPathReader::new(d);
    let mut cur = (0.0, 0.0);
    let mut start = (0.0, 0.0);
    let mut last_cp: Option<(f64, f64)> = None;
    let mut current_cmd = ' ';

    while reader.pos < reader.src.len() {
        if let Some(cmd) = reader.next_command() {
            current_cmd = cmd;
        } else if current_cmd == ' ' {
            break;
        }

        match current_cmd {
            'M' => {
                let mut first = true;
                while let (Some(x), Some(y)) = (reader.next_number(), reader.next_number()) {
                    cur = (x, y);
                    if first {
                        start = cur;
                        ops.push(PathOp::MoveTo(to_norm(x, vx), to_norm(y, vy)));
                        first = false;
                    } else {
                        ops.push(PathOp::LineTo(to_norm(x, vx), to_norm(y, vy)));
                    }
                }
                last_cp = None;
            }
            'm' => {
                let mut first = true;
                while let (Some(dx), Some(dy)) = (reader.next_number(), reader.next_number()) {
                    cur = (cur.0 + dx, cur.1 + dy);
                    if first {
                        start = cur;
                        ops.push(PathOp::MoveTo(to_norm(cur.0, vx), to_norm(cur.1, vy)));
                        first = false;
                    } else {
                        ops.push(PathOp::LineTo(to_norm(cur.0, vx), to_norm(cur.1, vy)));
                    }
                }
                last_cp = None;
            }
            'L' => {
                while let (Some(x), Some(y)) = (reader.next_number(), reader.next_number()) {
                    cur = (x, y);
                    ops.push(PathOp::LineTo(to_norm(x, vx), to_norm(y, vy)));
                }
                last_cp = None;
            }
            'l' => {
                while let (Some(dx), Some(dy)) = (reader.next_number(), reader.next_number()) {
                    cur = (cur.0 + dx, cur.1 + dy);
                    ops.push(PathOp::LineTo(to_norm(cur.0, vx), to_norm(cur.1, vy)));
                }
                last_cp = None;
            }
            'H' => {
                while let Some(x) = reader.next_number() {
                    cur.0 = x;
                    ops.push(PathOp::LineTo(to_norm(cur.0, vx), to_norm(cur.1, vy)));
                }
                last_cp = None;
            }
            'h' => {
                while let Some(dx) = reader.next_number() {
                    cur.0 += dx;
                    ops.push(PathOp::LineTo(to_norm(cur.0, vx), to_norm(cur.1, vy)));
                }
                last_cp = None;
            }
            'V' => {
                while let Some(y) = reader.next_number() {
                    cur.1 = y;
                    ops.push(PathOp::LineTo(to_norm(cur.0, vx), to_norm(cur.1, vy)));
                }
                last_cp = None;
            }
            'v' => {
                while let Some(dy) = reader.next_number() {
                    cur.1 += dy;
                    ops.push(PathOp::LineTo(to_norm(cur.0, vx), to_norm(cur.1, vy)));
                }
                last_cp = None;
            }
            'C' => {
                while let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                ) {
                    ops.push(PathOp::CubicTo([
                        to_norm(x1, vx), to_norm(y1, vy),
                        to_norm(x2, vx), to_norm(y2, vy),
                        to_norm(x, vx), to_norm(y, vy),
                    ]));
                    last_cp = Some((x2, y2));
                    cur = (x, y);
                }
            }
            'c' => {
                while let (Some(dx1), Some(dy1), Some(dx2), Some(dy2), Some(dx), Some(dy)) = (
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                ) {
                    let p1 = (cur.0 + dx1, cur.1 + dy1);
                    let p2 = (cur.0 + dx2, cur.1 + dy2);
                    let end = (cur.0 + dx, cur.1 + dy);
                    ops.push(PathOp::CubicTo([
                        to_norm(p1.0, vx), to_norm(p1.1, vy),
                        to_norm(p2.0, vx), to_norm(p2.1, vy),
                        to_norm(end.0, vx), to_norm(end.1, vy),
                    ]));
                    last_cp = Some(p2);
                    cur = end;
                }
            }
            'S' => {
                while let (Some(x2), Some(y2), Some(x), Some(y)) = (
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                ) {
                    let p1 = match last_cp {
                        Some(cp) => (2.0 * cur.0 - cp.0, 2.0 * cur.1 - cp.1),
                        None => cur,
                    };
                    ops.push(PathOp::CubicTo([
                        to_norm(p1.0, vx), to_norm(p1.1, vy),
                        to_norm(x2, vx), to_norm(y2, vy),
                        to_norm(x, vx), to_norm(y, vy),
                    ]));
                    last_cp = Some((x2, y2));
                    cur = (x, y);
                }
            }
            's' => {
                while let (Some(dx2), Some(dy2), Some(dx), Some(dy)) = (
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                ) {
                    let p1 = match last_cp {
                        Some(cp) => (2.0 * cur.0 - cp.0, 2.0 * cur.1 - cp.1),
                        None => cur,
                    };
                    let p2 = (cur.0 + dx2, cur.1 + dy2);
                    let end = (cur.0 + dx, cur.1 + dy);
                    ops.push(PathOp::CubicTo([
                        to_norm(p1.0, vx), to_norm(p1.1, vy),
                        to_norm(p2.0, vx), to_norm(p2.1, vy),
                        to_norm(end.0, vx), to_norm(end.1, vy),
                    ]));
                    last_cp = Some(p2);
                    cur = end;
                }
            }
            'Q' => {
                while let (Some(cx), Some(cy), Some(x), Some(y)) = (
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                ) {
                    ops.push(PathOp::QuadTo([
                        to_norm(cx, vx), to_norm(cy, vy),
                        to_norm(x, vx), to_norm(y, vy),
                    ]));
                    last_cp = Some((cx, cy));
                    cur = (x, y);
                }
            }
            'q' => {
                while let (Some(dcx), Some(dcy), Some(dx), Some(dy)) = (
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                ) {
                    let cp = (cur.0 + dcx, cur.1 + dcy);
                    let end = (cur.0 + dx, cur.1 + dy);
                    ops.push(PathOp::QuadTo([
                        to_norm(cp.0, vx), to_norm(cp.1, vy),
                        to_norm(end.0, vx), to_norm(end.1, vy),
                    ]));
                    last_cp = Some(cp);
                    cur = end;
                }
            }
            'T' => {
                while let (Some(x), Some(y)) = (reader.next_number(), reader.next_number()) {
                    let cp = match last_cp {
                        Some(p) => (2.0 * cur.0 - p.0, 2.0 * cur.1 - p.1),
                        None => cur,
                    };
                    ops.push(PathOp::QuadTo([
                        to_norm(cp.0, vx), to_norm(cp.1, vy),
                        to_norm(x, vx), to_norm(y, vy),
                    ]));
                    last_cp = Some(cp);
                    cur = (x, y);
                }
            }
            't' => {
                while let (Some(dx), Some(dy)) = (reader.next_number(), reader.next_number()) {
                    let cp = match last_cp {
                        Some(p) => (2.0 * cur.0 - p.0, 2.0 * cur.1 - p.1),
                        None => cur,
                    };
                    let end = (cur.0 + dx, cur.1 + dy);
                    ops.push(PathOp::QuadTo([
                        to_norm(cp.0, vx), to_norm(cp.1, vy),
                        to_norm(end.0, vx), to_norm(end.1, vy),
                    ]));
                    last_cp = Some(cp);
                    cur = end;
                }
            }
            'A' => {
                while let (Some(rx), Some(ry), Some(rot), Some(large), Some(sweep), Some(x), Some(y)) = (
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                ) {
                    arc_to_beziers(
                        cur.0, cur.1,
                        rx, ry, rot,
                        large != 0.0, sweep != 0.0,
                        x, y,
                        vx, vy,
                        ops,
                    );
                    last_cp = None;
                    cur = (x, y);
                }
            }
            'a' => {
                while let (Some(rx), Some(ry), Some(rot), Some(large), Some(sweep), Some(dx), Some(dy)) = (
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                    reader.next_number(),
                ) {
                    let end = (cur.0 + dx, cur.1 + dy);
                    arc_to_beziers(
                        cur.0, cur.1,
                        rx, ry, rot,
                        large != 0.0, sweep != 0.0,
                        end.0, end.1,
                        vx, vy,
                        ops,
                    );
                    last_cp = None;
                    cur = end;
                }
            }
            'Z' | 'z' => {
                ops.push(PathOp::Close);
                cur = start;
                last_cp = None;
            }
            _ => {
                last_cp = None;
            }
        }
    }
}

fn arc_to_beziers(
    x1: f64, y1: f64,
    mut rx: f64, mut ry: f64,
    phi_deg: f64,
    large_arc: bool, sweep: bool,
    x2: f64, y2: f64,
    vx: f64, vy: f64,
    ops: &mut Vec<PathOp>,
) {
    if (x1 - x2).abs() < 1e-6 && (y1 - y2).abs() < 1e-6 {
        return;
    }
    if rx.abs() < 1e-6 || ry.abs() < 1e-6 {
        ops.push(PathOp::LineTo(to_norm(x2, vx), to_norm(y2, vy)));
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let phi = phi_deg.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1_prime = cos_phi * dx + sin_phi * dy;
    let y1_prime = -sin_phi * dx + cos_phi * dy;

    let cr = (x1_prime * x1_prime) / (rx * rx) + (y1_prime * y1_prime) / (ry * ry);
    if cr > 1.0 {
        let s = cr.sqrt();
        rx *= s;
        ry *= s;
    }

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let num = (rx * rx * ry * ry) - (rx * rx * y1_prime * y1_prime) - (ry * ry * x1_prime * x1_prime);
    let den = (rx * rx * y1_prime * y1_prime) + (ry * ry * x1_prime * x1_prime);
    let factor = sign * (num.max(0.0) / den.max(1e-12)).sqrt();
    let cx_prime = factor * (rx * y1_prime) / ry;
    let cy_prime = factor * -(ry * x1_prime) / rx;

    let cx = cos_phi * cx_prime - sin_phi * cy_prime + (x1 + x2) / 2.0;
    let cy = sin_phi * cx_prime + cos_phi * cy_prime + (y1 + y2) / 2.0;

    let v_x = (x1_prime - cx_prime) / rx;
    let v_y = (y1_prime - cy_prime) / ry;
    let u_x = (-x1_prime - cx_prime) / rx;
    let u_y = (-y1_prime - cy_prime) / ry;

    let theta1 = v_y.atan2(v_x);
    let mut delta_theta = (v_x * u_y - v_y * u_x).atan2(v_x * u_x + v_y * u_y);
    if !sweep && delta_theta > 0.0 {
        delta_theta -= std::f64::consts::TAU;
    } else if sweep && delta_theta < 0.0 {
        delta_theta += std::f64::consts::TAU;
    }

    let segments = (delta_theta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let d_theta = delta_theta / segments as f64;
    let mut t = theta1;

    for _ in 0..segments {
        let t_next = t + d_theta;
        let alpha = d_theta.sin() * ((4.0 + 3.0 * (d_theta / 2.0).tan().powi(2)).sqrt() - 1.0) / 3.0;

        let cos_t = t.cos();
        let sin_t = t.sin();
        let cos_t_next = t_next.cos();
        let sin_t_next = t_next.sin();

        let ep1_x = rx * cos_t;
        let ep1_y = ry * sin_t;
        let ep2_x = rx * cos_t_next;
        let ep2_y = ry * sin_t_next;

        let cp1_x = ep1_x - alpha * rx * sin_t;
        let cp1_y = ep1_y + alpha * ry * cos_t;
        let cp2_x = ep2_x + alpha * rx * sin_t_next;
        let cp2_y = ep2_y - alpha * ry * cos_t_next;

        let tr = |x: f64, y: f64| -> (f64, f64) {
            (cos_phi * x - sin_phi * y + cx, sin_phi * x + cos_phi * y + cy)
        };
        let p_cp1 = tr(cp1_x, cp1_y);
        let p_cp2 = tr(cp2_x, cp2_y);
        let p_end = tr(ep2_x, ep2_y);

        ops.push(PathOp::CubicTo([
            to_norm(p_cp1.0, vx), to_norm(p_cp1.1, vy),
            to_norm(p_cp2.0, vx), to_norm(p_cp2.1, vy),
            to_norm(p_end.0, vx), to_norm(p_end.1, vy),
        ]));

        t = t_next;
    }
}

#[cfg(test)]
mod tests {

    /// A colour with a multi-byte character is not a colour (it was sliced by byte position and panicked).
    #[test]
    fn colour_with_multibyte_character_is_none() {
        assert!(parse_color_rgb("#\u{e9}1").is_none());
        assert!(parse_color_rgb("#1\u{e9}").is_none());
        assert_eq!(parse_color_rgb("#abc").map(|c| c.0), Some(0xaabbcc));
    }
    use super::*;
    use ibis_doc_model::{EmuSize, Presentation};

    #[test]
    fn test_convert_simple_svg() {
        let mut pres = Presentation::empty(EmuSize::WIDESCREEN);
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100">
            <rect x="10" y="10" width="80" height="80" fill="#ff0000"/>
        </svg>"##;
        let frame = EmuRect::new(0, 0, 1_000_000, 1_000_000);
        let shapes = convert_svg_to_shapes(|| pres.new_shape_id(), svg.as_bytes(), frame).expect("convert succeeds");
        assert_eq!(shapes.len(), 1);
        let shape = &shapes[0];
        let ShapeKind::Auto(auto) = &shape.kind else { panic!("expected AutoShape") };
        assert_eq!(auto.fill, Fill::Solid(Color::opaque(Rgb(0xff0000))));
        assert!(matches!(auto.geometry, Geometry::Formula(_)));
    }

    #[test]
    fn test_convert_multi_element_svg() {
        let mut pres = Presentation::empty(EmuSize::WIDESCREEN);
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200">
            <circle cx="50" cy="50" r="40" fill="blue"/>
            <path d="M 10 10 L 90 90 Z" fill="none" stroke="red" stroke-width="2"/>
        </svg>"##;
        let frame = EmuRect::new(100, 200, 500_000, 500_000);
        let shapes = convert_svg_to_shapes(|| pres.new_shape_id(), svg.as_bytes(), frame).expect("convert succeeds");
        assert_eq!(shapes.len(), 2);
    }
}
