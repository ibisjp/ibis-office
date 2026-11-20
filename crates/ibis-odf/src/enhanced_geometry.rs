// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The geometry of a `draw:custom-shape` (`draw:enhanced-geometry`, OASIS ODF 1.3 section 20.209 and 19.145) as DrawingML custom
//! geometry: the equations (`draw:equation`, `?fN`), the modifiers (`$N`), the identifiers (`left`, `width`, `pi` ...) and the
//! commands of `draw:enhanced-path` (M L C Q X Y A B W V T U Z N F S) are evaluated for the shape's size, and the resulting
//! paths are written as `a:path` elements with plain coordinates (arcs as cubic Bezier curves).

use std::collections::HashMap;
use std::fmt::Write;

use roxmltree::Node;

fn attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

/// The evaluated geometry: the path space, the paths and the text area.
pub struct Geometry {
    pub path_w: i64,
    pub path_h: i64,
    /// `<a:path>` elements.
    pub paths: String,
    /// The text area (`draw:text-areas`) as fractions of the view box (left, top, right, bottom), when the shape has one.
    pub text_area: Option<(f64, f64, f64, f64)>,
}

struct Env<'a> {
    formulas: HashMap<String, &'a str>,
    modifiers: Vec<f64>,
    cache: HashMap<String, f64>,
    busy: Vec<String>,
    view: (f64, f64, f64, f64),
    size: (f64, f64),
    stretch: (f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(char),
}

fn tokenize(s: &str) -> Vec<Tok> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || ch == '.' {
            let st = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') && i + 1 < c.len() && (c[i + 1].is_ascii_digit() || c[i + 1] == '-' || c[i + 1] == '+') {
                i += 2;
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(Tok::Num(c[st..i].iter().collect::<String>().parse().unwrap_or(0.0)));
        } else if ch == '?' || ch == '$' || ch.is_ascii_alphabetic() {
            let st = i;
            i += 1;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Name(c[st..i].iter().collect()));
        } else {
            out.push(Tok::Op(ch));
            i += 1;
        }
    }
    out
}

struct Parser<'e, 'a> {
    toks: Vec<Tok>,
    pos: usize,
    env: &'e mut Env<'a>,
}

impl Parser<'_, '_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn expr(&mut self) -> f64 {
        let mut v = self.term();
        while let Some(Tok::Op(o)) = self.peek().cloned() {
            if o == '+' {
                self.pos += 1;
                v += self.term();
            } else if o == '-' {
                self.pos += 1;
                v -= self.term();
            } else {
                break;
            }
        }
        v
    }
    fn term(&mut self) -> f64 {
        let mut v = self.factor();
        while let Some(Tok::Op(o)) = self.peek().cloned() {
            if o == '*' {
                self.pos += 1;
                v *= self.factor();
            } else if o == '/' {
                self.pos += 1;
                let d = self.factor();
                v = if d == 0.0 { 0.0 } else { v / d };
            } else {
                break;
            }
        }
        v
    }
    fn factor(&mut self) -> f64 {
        match self.peek().cloned() {
            Some(Tok::Op('-')) => {
                self.pos += 1;
                -self.factor()
            }
            Some(Tok::Op('+')) => {
                self.pos += 1;
                self.factor()
            }
            Some(Tok::Op('(')) => {
                self.pos += 1;
                let v = self.expr();
                if self.peek() == Some(&Tok::Op(')')) {
                    self.pos += 1;
                }
                v
            }
            Some(Tok::Num(n)) => {
                self.pos += 1;
                n
            }
            Some(Tok::Name(name)) => {
                self.pos += 1;
                if self.peek() == Some(&Tok::Op('(')) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    while self.peek().is_some() && self.peek() != Some(&Tok::Op(')')) {
                        args.push(self.expr());
                        if self.peek() == Some(&Tok::Op(',')) {
                            self.pos += 1;
                        }
                    }
                    if self.peek() == Some(&Tok::Op(')')) {
                        self.pos += 1;
                    }
                    call(&name, &args)
                } else {
                    self.env.value(&name)
                }
            }
            _ => {
                self.pos += 1;
                0.0
            }
        }
    }
}

fn call(name: &str, a: &[f64]) -> f64 {
    let g = |i: usize| a.get(i).copied().unwrap_or(0.0);
    match name {
        "abs" => g(0).abs(),
        "sqrt" => g(0).max(0.0).sqrt(),
        "sin" => g(0).sin(),
        "cos" => g(0).cos(),
        "tan" => g(0).tan(),
        "atan" => g(0).atan(),
        "atan2" => g(0).atan2(g(1)),
        "min" => g(0).min(g(1)),
        "max" => g(0).max(g(1)),
        "if" => {
            if g(0) > 0.0 {
                g(1)
            } else {
                g(2)
            }
        }
        _ => 0.0,
    }
}

impl<'a> Env<'a> {
    fn value(&mut self, name: &str) -> f64 {
        if let Some(rest) = name.strip_prefix('?') {
            return self.equation(rest);
        }
        if let Some(rest) = name.strip_prefix('$') {
            return rest.parse::<usize>().ok().and_then(|i| self.modifiers.get(i).copied()).unwrap_or(0.0);
        }
        let (vx, vy, vw, vh) = self.view;
        match name {
            "pi" => std::f64::consts::PI,
            "left" => vx,
            "top" => vy,
            "right" => vx + vw,
            "bottom" => vy + vh,
            "xstretch" => self.stretch.0,
            "ystretch" => self.stretch.1,
            "hasstroke" | "hasfill" => 1.0,
            "width" => self.size.0,
            "height" => self.size.1,
            "logwidth" => vw,
            "logheight" => vh,
            _ => 0.0,
        }
    }

    fn equation(&mut self, name: &str) -> f64 {
        if let Some(v) = self.cache.get(name) {
            return *v;
        }
        if self.busy.iter().any(|b| b == name) {
            return 0.0;
        }
        let Some(&formula) = self.formulas.get(name) else { return 0.0 };
        self.busy.push(name.to_string());
        let toks = tokenize(formula);
        let mut p = Parser { toks, pos: 0, env: self };
        let v = p.expr();
        self.busy.pop();
        let v = if v.is_finite() { v } else { 0.0 };
        self.cache.insert(name.to_string(), v);
        v
    }

    /// A value written in a path or a list: a number, `?fN`, `$N` or an identifier, or a whole formula.
    fn eval(&mut self, text: &str) -> f64 {
        let toks = tokenize(text);
        let mut p = Parser { toks, pos: 0, env: self };
        let v = p.expr();
        if v.is_finite() {
            v
        } else {
            0.0
        }
    }
}

type Pt = (f64, f64);

fn pt_xml(out: &mut String, p: Pt, o: Pt, s: f64) {
    let _ = write!(out, r#"<a:pt x="{}" y="{}"/>"#, ((p.0 - o.0) * s).round() as i64, ((p.1 - o.1) * s).round() as i64);
}

/// A counterclockwise (on the screen, y down: decreasing parameter) or clockwise elliptical arc from parameter `a0` to `a1`
/// as cubic Bezier segments of at most a quarter turn.
fn arc_curves(c: Pt, r: Pt, a0: f64, sweep: f64) -> Vec<(Pt, Pt, Pt)> {
    let n = ((sweep.abs() / std::f64::consts::FRAC_PI_2 - 1e-6).ceil() as usize).max(1);
    let step = sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |t: f64| (c.0 + r.0 * t.cos(), c.1 + r.1 * t.sin());
    let d = |t: f64| (-r.0 * t.sin(), r.1 * t.cos());
    (0..n)
        .map(|i| {
            let t0 = a0 + step * i as f64;
            let t1 = t0 + step;
            let (p0, p3) = (at(t0), at(t1));
            let (d0, d1) = (d(t0), d(t1));
            ((p0.0 + k * d0.0, p0.1 + k * d0.1), (p3.0 - k * d1.0, p3.1 - k * d1.1), p3)
        })
        .collect()
}

#[derive(Default)]
struct PathBuilder {
    cmds: String,
    cur: Pt,
    fill: bool,
    stroke: bool,
    open: bool,
}

/// The factor of the literal number that is argument `i` of path command `cmd` (see [`evaluate_in`]): x and y alternate, the radii of
/// `T`/`U` follow, their angles are not lengths.
fn literal_factor(cmd: char, i: usize, lit: (f64, f64)) -> f64 {
    match cmd {
        'M' | 'L' | 'C' | 'Q' | 'X' | 'Y' | 'A' | 'B' | 'W' | 'V' => if i % 2 == 0 { lit.0 } else { lit.1 },
        'T' | 'U' => match i % 6 {
            0 | 2 => lit.0,
            1 | 3 => lit.1,
            _ => 1.0,
        },
        _ => 1.0,
    }
}

/// `draw:enhanced-path` of the geometry `eg` for a shape of `size` (in the units of the shape's `svg:width`/`svg:height`); with `excel` a geometry that has `draw:path-stretchpoint-x`/`-y` is evaluated as Excel reads an .ods: the
/// equations' `right` and `bottom` are `viewBox width * w / ss` and `viewBox height * h / ss` (`ss` the shorter side of the shape), and the
/// path is drawn in that box, so one unit of the equations is `ss / viewBox width` of the shape in both directions (the drawing keeps
/// its proportions; the corners of a rounded shape stay circles). A geometry without them is stretched, `right`/`bottom` being
/// the view box's. Measured with `-SaveAs toxlsx` (Excel writes what it read as an `a:custGeom`): with the attribute the
/// `gdLst` has `right = 21600 * w / ss` and the scale `min(w / 21600, h / 21600)`, the viewBox 21600 x 10800 and 10800 x 21600 and
/// 5000 x 5000 change those numbers the same way; without it `right = 21600` and the points are scaled `w / 21600`, `h / 10800`
/// separately (a literal path such as `M 3590 0 X 0 3590 ...` and a path of `?fN`; the attribute is the only thing that
/// differs, adding it to a literal path changes the formulas to the first form and removing it from a path of `?fN` to the second).
pub fn evaluate_in(eg: Node, size: (f64, f64), excel: bool) -> Option<Geometry> {
    let mut vb: Vec<f64> = attr(eg, "viewBox")?.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    // The factors of the literal coordinates of the path (x, y) in that reading: `right` is `viewBox width * w / ss`, and a literal
    // `21600` in x is `right` (measured: the rounded rectangle `M ?f7 0 X 0 ?f8 L 0 ?f9 Y ?f7 21600 ... X 21600 ?f9`, whose
    // equations use `right - $0`, is saved as arcs from `right`/`bottom` to the corners, and the shape is drawn full width), so the literals are
    // `x * w / ss` and `y * h / ss`.
    let mut lit = (1.0, 1.0);
    if excel && vb.len() == 4 && (attr(eg, "path-stretchpoint-x").is_some() || attr(eg, "path-stretchpoint-y").is_some()) {
        let ss = size.0.min(size.1);
        if ss > 0.0 {
            lit = (size.0 / ss, size.1 / ss);
            vb[2] *= lit.0;
            vb[3] *= lit.1;
        }
    }
    if vb.len() != 4 || vb[2] <= 0.0 || vb[3] <= 0.0 {
        return None;
    }
    let path = attr(eg, "enhanced-path")?;
    let mut formulas = HashMap::new();
    for e in eg.children().filter(|c| c.is_element() && c.tag_name().name() == "equation") {
        if let (Some(n), Some(f)) = (attr(e, "name"), attr(e, "formula")) {
            formulas.insert(n.to_string(), f);
        }
    }
    let modifiers: Vec<f64> = attr(eg, "modifiers").map(|m| m.split_whitespace().filter_map(|t| t.parse().ok()).collect()).unwrap_or_default();
    let stretch = (attr(eg, "path-stretchpoint-x").and_then(|v| v.parse().ok()).unwrap_or(vb[2] / 2.0), attr(eg, "path-stretchpoint-y").and_then(|v| v.parse().ok()).unwrap_or(vb[3] / 2.0));
    let mut env = Env { formulas, modifiers, cache: HashMap::new(), busy: Vec::new(), view: (vb[0], vb[1], vb[2], vb[3]), size, stretch };
    let origin = (vb[0], vb[1]);
    let big = vb[2].max(vb[3]);
    let scale = (100_000.0 / big).floor().max(1.0);

    let mut b = PathBuilder { fill: true, stroke: true, ..Default::default() };
    let mut toks = path.split_whitespace().peekable();
    let mut cmd = ' ';
    let mut args: Vec<f64> = Vec::new();
    let mut all: Vec<(char, Vec<f64>)> = Vec::new();
    while let Some(t) = toks.next() {
        let first = t.chars().next()?;
        let is_cmd = first.is_ascii_alphabetic() && t.chars().all(|c| c.is_ascii_alphabetic()) && !matches!(t, "left" | "top" | "right" | "bottom" | "pi" | "width" | "height" | "xstretch" | "ystretch" | "logwidth" | "logheight" | "hasstroke" | "hasfill");
        if is_cmd && t.len() == 1 {
            if cmd != ' ' {
                all.push((cmd, std::mem::take(&mut args)));
            }
            cmd = first;
            if matches!(cmd, 'Z' | 'N' | 'F' | 'S') {
                all.push((cmd, Vec::new()));
                cmd = ' ';
            }
        } else if is_cmd {
            // Two-letter commands of the old format (AT, AR, WA, WR, NF, NS ...) are not supported.
            return None;
        } else {
            let k = if t.parse::<f64>().is_ok() { literal_factor(cmd, args.len(), lit) } else { 1.0 };
            args.push(env.eval(t) * k);
        }
    }
    if cmd != ' ' {
        all.push((cmd, args));
    }

    let mut out_paths = String::new();
    let flush = |b: &mut PathBuilder, out: &mut String| {
        if !b.cmds.is_empty() {
            let _ = write!(out, r#"<a:path w="{}" h="{}"{}{}>{}</a:path>"#, (vb[2] * scale).round() as i64, (vb[3] * scale).round() as i64, if b.fill { "" } else { r#" fill="none""# }, if b.stroke { "" } else { r#" stroke="0""# }, b.cmds);
        }
        *b = PathBuilder { fill: true, stroke: true, ..Default::default() };
    };
    for (c, a) in all {
        match c {
            'M' => {
                for (i, p) in a.chunks(2).filter(|p| p.len() == 2).enumerate() {
                    let tag = if i == 0 { "moveTo" } else { "lnTo" };
                    let _ = write!(b.cmds, "<a:{tag}>");
                    pt_xml(&mut b.cmds, (p[0], p[1]), origin, scale);
                    let _ = write!(b.cmds, "</a:{tag}>");
                    b.cur = (p[0], p[1]);
                    b.open = true;
                }
            }
            'L' => {
                for p in a.chunks(2).filter(|p| p.len() == 2) {
                    b.cmds.push_str("<a:lnTo>");
                    pt_xml(&mut b.cmds, (p[0], p[1]), origin, scale);
                    b.cmds.push_str("</a:lnTo>");
                    b.cur = (p[0], p[1]);
                }
            }
            'C' => {
                for p in a.chunks(6).filter(|p| p.len() == 6) {
                    b.cmds.push_str("<a:cubicBezTo>");
                    for i in 0..3 {
                        pt_xml(&mut b.cmds, (p[2 * i], p[2 * i + 1]), origin, scale);
                    }
                    b.cmds.push_str("</a:cubicBezTo>");
                    b.cur = (p[4], p[5]);
                }
            }
            'Q' => {
                for p in a.chunks(4).filter(|p| p.len() == 4) {
                    b.cmds.push_str("<a:quadBezTo>");
                    pt_xml(&mut b.cmds, (p[0], p[1]), origin, scale);
                    pt_xml(&mut b.cmds, (p[2], p[3]), origin, scale);
                    b.cmds.push_str("</a:quadBezTo>");
                    b.cur = (p[2], p[3]);
                }
            }
            'X' | 'Y' => {
                const K: f64 = 0.552_284_749_8;
                for p in a.chunks(2).filter(|p| p.len() == 2) {
                    let (x0, y0) = b.cur;
                    let (x, y) = (p[0], p[1]);
                    let (c1, c2) = if c == 'X' { ((x0 + K * (x - x0), y0), (x, y - K * (y - y0))) } else { ((x0, y0 + K * (y - y0)), (x - K * (x - x0), y)) };
                    b.cmds.push_str("<a:cubicBezTo>");
                    pt_xml(&mut b.cmds, c1, origin, scale);
                    pt_xml(&mut b.cmds, c2, origin, scale);
                    pt_xml(&mut b.cmds, (x, y), origin, scale);
                    b.cmds.push_str("</a:cubicBezTo>");
                    b.cur = (x, y);
                }
            }
            'A' | 'B' | 'W' | 'V' => {
                let clockwise = c == 'W' || c == 'V';
                let to = c == 'A' || c == 'W';
                for p in a.chunks(8).filter(|p| p.len() == 8) {
                    let (cx, cy) = ((p[0] + p[2]) / 2.0, (p[1] + p[3]) / 2.0);
                    let (rx, ry) = ((p[2] - p[0]).abs() / 2.0, (p[3] - p[1]).abs() / 2.0);
                    if rx <= 0.0 || ry <= 0.0 {
                        continue;
                    }
                    let t0 = ((p[5] - cy) / ry).atan2((p[4] - cx) / rx);
                    let t1 = ((p[7] - cy) / ry).atan2((p[6] - cx) / rx);
                    let tau = std::f64::consts::TAU;
                    let mut sweep = if clockwise { (t1 - t0).rem_euclid(tau) } else { -(t0 - t1).rem_euclid(tau) };
                    if sweep.abs() < 1e-9 {
                        sweep = if clockwise { tau } else { -tau };
                    }
                    let start = (cx + rx * t0.cos(), cy + ry * t0.sin());
                    let tag = if to && b.open { "lnTo" } else { "moveTo" };
                    let _ = write!(b.cmds, "<a:{tag}>");
                    pt_xml(&mut b.cmds, start, origin, scale);
                    let _ = write!(b.cmds, "</a:{tag}>");
                    b.open = true;
                    for (c1, c2, e) in arc_curves((cx, cy), (rx, ry), t0, sweep) {
                        b.cmds.push_str("<a:cubicBezTo>");
                        pt_xml(&mut b.cmds, c1, origin, scale);
                        pt_xml(&mut b.cmds, c2, origin, scale);
                        pt_xml(&mut b.cmds, e, origin, scale);
                        b.cmds.push_str("</a:cubicBezTo>");
                        b.cur = e;
                    }
                }
            }
            'T' | 'U' => {
                // x y wR hR startAngle endAngle (degrees): an arc of the ellipse centred at (x, y); T joins it to the current point.
                let to = c == 'T';
                for p in a.chunks(6).filter(|p| p.len() == 6) {
                    let (rx, ry) = (p[2].abs(), p[3].abs());
                    if rx <= 0.0 || ry <= 0.0 {
                        continue;
                    }
                    let (t0, t1) = (p[4].to_radians(), p[5].to_radians());
                    let start = (p[0] + rx * t0.cos(), p[1] + ry * t0.sin());
                    let tag = if to && b.open { "lnTo" } else { "moveTo" };
                    let _ = write!(b.cmds, "<a:{tag}>");
                    pt_xml(&mut b.cmds, start, origin, scale);
                    let _ = write!(b.cmds, "</a:{tag}>");
                    b.open = true;
                    for (c1, c2, e) in arc_curves((p[0], p[1]), (rx, ry), t0, t1 - t0) {
                        b.cmds.push_str("<a:cubicBezTo>");
                        pt_xml(&mut b.cmds, c1, origin, scale);
                        pt_xml(&mut b.cmds, c2, origin, scale);
                        pt_xml(&mut b.cmds, e, origin, scale);
                        b.cmds.push_str("</a:cubicBezTo>");
                        b.cur = e;
                    }
                }
            }
            'Z' => {
                b.cmds.push_str("<a:close/>");
                b.open = false;
            }
            'F' => b.fill = false,
            'S' => b.stroke = false,
            'N' => flush(&mut b, &mut out_paths),
            _ => return None,
        }
    }
    flush(&mut b, &mut out_paths);
    if out_paths.is_empty() {
        return None;
    }
    let text_area = attr(eg, "text-areas").map(|t| {
        let parts: Vec<&str> = t.split_whitespace().collect();
        let v: Vec<f64> = parts.iter().enumerate().map(|(i, p)| env.eval(p) * if p.parse::<f64>().is_ok() { if i % 2 == 0 { lit.0 } else { lit.1 } } else { 1.0 }).collect();
        v
    });
    let text_area = text_area.filter(|v| v.len() == 4).map(|v| ((v[0] - vb[0]) / vb[2], (v[1] - vb[1]) / vb[3], (v[2] - vb[0]) / vb[2], (v[3] - vb[1]) / vb[3]));
    Some(Geometry { path_w: (vb[2] * scale).round() as i64, path_h: (vb[3] * scale).round() as i64, paths: out_paths, text_area })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracket_pair_corners_are_quarter_arcs() {
        let text = include_str!("../tests/data/bracket_pair.xml");
        let doc = roxmltree::Document::parse(text).unwrap();
        let eg = doc.descendants().find(|n| n.tag_name().name() == "enhanced-geometry").unwrap();
        let g = evaluate_in(eg, (4000.0, 800.0), false).unwrap();
        // Each corner is a quarter turn: four Bezier segments make a full circle, so a quarter has one.
        assert_eq!(g.paths.matches("<a:cubicBezTo>").count(), 8, "{}", g.paths);
    }

    #[test]
    fn stretch_point_geometry_is_drawn_in_the_aspect_box_for_excel() {
        // A rounded rectangle of the older format: literal 0 and 21600 beside `right - $0` (`?f10`).
        let xml = r#"<d xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"><draw:enhanced-geometry draw:path-stretchpoint-x="10800" draw:path-stretchpoint-y="10800" svg:viewBox="0 0 21600 21600" draw:enhanced-path="M ?f7 0 L ?f10 0 L 21600 21600 Z N" draw:modifiers="3600"><draw:equation draw:name="f7" draw:formula="left+$0 "/><draw:equation draw:name="f10" draw:formula="right-$0 "/></draw:enhanced-geometry></d>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let eg = doc.descendants().find(|n| n.tag_name().name() == "enhanced-geometry").unwrap();
        // A shape three times as wide as high: right = 3 * 21600, the corner (3600) stays 3600, the literal 21600 is `right`.
        let g = evaluate_in(eg, (3000.0, 1000.0), true).unwrap();
        assert_eq!(g.path_w, 3 * g.path_h, "{}", g.paths);
        let scale = g.path_h as f64 / 21600.0;
        assert!(g.paths.contains(&format!("x=\"{}\" y=\"0\"", (3.0 * 21600.0 - 3600.0) * scale)), "{}", g.paths);
        assert!(g.paths.contains(&format!("x=\"{}\" y=\"{}\"", 3.0 * 21600.0 * scale, 21600.0 * scale)), "{}", g.paths);
        // Without the flag the shape is stretched: right is 21600.
        let g = evaluate_in(eg, (3000.0, 1000.0), false).unwrap();
        assert_eq!(g.path_w, g.path_h);
    }
}
