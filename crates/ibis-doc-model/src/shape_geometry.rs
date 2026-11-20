// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DrawingML shape geometry: guide formulas and paths.
//!
//! Preset shapes (`a:prstGeom`) and custom shapes (`a:custGeom`) describe
//! their outline with named guides computed from the shape's size and
//! adjust values, and paths whose points refer to those guides. The outline
//! is computed for the size a shape is drawn at, so it follows resizing.

use std::collections::HashMap;
use std::sync::Arc;

/// A named value and the formula that computes it, such as
/// `("x1", "*/ w adj1 100000")`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guide {
    pub name: String,
    pub formula: String,
}

/// One step of a geometry path. Arguments are guide names or numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathOp {
    MoveTo(String, String),
    LineTo(String, String),
    /// Elliptical arc from the current point: radii, start angle and sweep
    /// (60 000ths of a degree), as `a:arcTo`.
    ArcTo { wr: String, hr: String, start: String, sweep: String },
    QuadTo([String; 4]),
    CubicTo([String; 6]),
    Close,
}

/// How a path is filled, as `a:path fill`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathFill {
    #[default]
    Normal,
    None,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeometryPath {
    /// Coordinate space of the points; 0 means the shape's own size.
    pub width: i64,
    pub height: i64,
    pub fill: PathFill,
    pub stroke: bool,
    pub ops: Vec<PathOp>,
}

/// A shape's geometry definition: default adjust values, guides and paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GeometryDef {
    pub adjust: Vec<Guide>,
    pub guides: Vec<Guide>,
    pub paths: Vec<GeometryPath>,
    /// The text rectangle (`a:rect`): left, top, right and bottom, as guide
    /// names or formulas' values; `None` is the whole shape.
    pub text_rect: Option<[String; 4]>,
}

/// The geometry of one shape: a definition (shared between shapes using the
/// same preset) and the shape's own adjust values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaGeometry {
    /// The preset's name (`bentUpArrow`), or `None` for a custom geometry.
    pub preset: Option<String>,
    pub adjust: Vec<Guide>,
    pub def: Arc<GeometryDef>,
}

/// A path computed for a size: subpaths of points in shape coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub subpaths: Vec<Subpath>,
    pub fill: PathFill,
    pub stroke: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Subpath {
    pub points: Vec<(f64, f64)>,
    pub closed: bool,
}

/// A vertex of an outline with its Bézier handles (頂点の編集): the point, and the control points of the curve coming
/// into it and going out of it (`None`: a straight line there).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub p: (f64, f64),
    pub cin: Option<(f64, f64)>,
    pub cout: Option<(f64, f64)>,
}

impl Node {
    pub fn corner(p: (f64, f64)) -> Self {
        Node { p, cin: None, cout: None }
    }
}

impl FormulaGeometry {
    /// The closed figures of the outline for a shape `w` × `h`, as vertices with their curves, in the shape's
    /// coordinates; `None` if a path has an arc or is not closed (the outline can be had as lines only).
    pub fn nodes(&self, w: f64, h: f64) -> Option<Vec<Vec<Node>>> {
        let mut v = Values::new(w, h);
        v.set_all(&self.def.adjust);
        v.set_all(&self.adjust);
        v.set_all(&self.def.guides);
        let mut out = Vec::new();
        for path in &self.def.paths {
            if path.fill == PathFill::None {
                continue;
            }
            let sx = if path.width > 0 { w / path.width as f64 } else { 1.0 };
            let sy = if path.height > 0 { h / path.height as f64 } else { 1.0 };
            let pt = |x: &str, y: &str| (v.get(x) * sx, v.get(y) * sy);
            let mut current: Vec<Node> = Vec::new();
            for op in &path.ops {
                match op {
                    PathOp::MoveTo(x, y) => {
                        if !current.is_empty() {
                            return None;
                        }
                        current.push(Node::corner(pt(x, y)));
                    }
                    PathOp::LineTo(x, y) => current.push(Node::corner(pt(x, y))),
                    PathOp::QuadTo([cx, cy, x, y]) => {
                        let (c, t) = (pt(cx, cy), pt(x, y));
                        let last = current.last_mut()?;
                        let p0 = last.p;
                        last.cout = Some((p0.0 + 2.0 / 3.0 * (c.0 - p0.0), p0.1 + 2.0 / 3.0 * (c.1 - p0.1)));
                        current.push(Node { p: t, cin: Some((t.0 + 2.0 / 3.0 * (c.0 - t.0), t.1 + 2.0 / 3.0 * (c.1 - t.1))), cout: None });
                    }
                    PathOp::CubicTo([ax, ay, bx, by, x, y]) => {
                        let (c1, c2, t) = (pt(ax, ay), pt(bx, by), pt(x, y));
                        // A handle on its own vertex is none (a line at that side).
                        let apart = |h: (f64, f64), p: (f64, f64)| ((h.0 - p.0).abs() >= 0.5 || (h.1 - p.1).abs() >= 0.5).then_some(h);
                        let last = current.last_mut()?;
                        last.cout = apart(c1, last.p);
                        current.push(Node { p: t, cin: apart(c2, t), cout: None });
                    }
                    PathOp::ArcTo { .. } => return None,
                    PathOp::Close => {
                        // The last point on the first: one vertex (a curve closing on it comes into it).
                        if current.len() >= 2 {
                            let (first, last) = (current[0], *current.last()?);
                            if (first.p.0 - last.p.0).abs() < 0.5 && (first.p.1 - last.p.1).abs() < 0.5 {
                                current[0].cin = last.cin;
                                current.pop();
                            }
                        }
                        if current.len() >= 3 {
                            out.push(std::mem::take(&mut current));
                        } else {
                            current.clear();
                        }
                    }
                }
            }
            if !current.is_empty() {
                return None;
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// 60 000ths of a degree to radians.
fn radians(angle: f64) -> f64 {
    (angle / 60_000.0).to_radians()
}

fn angle_of(r: f64) -> f64 {
    r.to_degrees() * 60_000.0
}

struct Values {
    values: HashMap<String, f64>,
}

impl Values {
    /// The built-in variables for a shape `w` × `h` (ECMA-376 §20.1.10.56).
    fn new(w: f64, h: f64) -> Self {
        let ss = w.min(h);
        let mut values = HashMap::new();
        let mut set = |name: &str, v: f64| {
            values.insert(name.to_string(), v);
        };
        set("w", w);
        set("h", h);
        set("l", 0.0);
        set("t", 0.0);
        set("r", w);
        set("b", h);
        set("hc", w / 2.0);
        set("vc", h / 2.0);
        set("ss", ss);
        set("ls", w.max(h));
        for d in [2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 32.0] {
            set(&format!("wd{d}"), w / d);
            set(&format!("hd{d}"), h / d);
        }
        for d in [2.0, 4.0, 6.0, 8.0, 16.0, 32.0] {
            set(&format!("ssd{d}"), ss / d);
        }
        set("cd2", 10_800_000.0);
        set("cd4", 5_400_000.0);
        set("cd8", 2_700_000.0);
        set("3cd4", 16_200_000.0);
        set("3cd8", 8_100_000.0);
        set("5cd8", 13_500_000.0);
        set("7cd8", 18_900_000.0);
        Self { values }
    }

    fn get(&self, arg: &str) -> f64 {
        match self.values.get(arg) {
            Some(v) => *v,
            None => arg.parse().unwrap_or(0.0),
        }
    }

    /// Evaluate one guide formula (ECMA-376 §20.1.9.11).
    fn eval(&self, formula: &str) -> f64 {
        let mut parts = formula.split_whitespace();
        let op = parts.next().unwrap_or("val");
        let a: Vec<f64> = parts.map(|p| self.get(p)).collect();
        let arg = |i: usize| a.get(i).copied().unwrap_or(0.0);
        let (x, y, z) = (arg(0), arg(1), arg(2));
        match op {
            "val" => x,
            "*/" => if z == 0.0 { 0.0 } else { x * y / z },
            "+-" => x + y - z,
            "+/" => if z == 0.0 { 0.0 } else { (x + y) / z },
            "?:" => if x > 0.0 { y } else { z },
            "abs" => x.abs(),
            "at2" => angle_of(y.atan2(x)),
            "cat2" => x * z.atan2(y).cos(),
            "sat2" => x * z.atan2(y).sin(),
            "cos" => x * radians(y).cos(),
            "sin" => x * radians(y).sin(),
            "tan" => x * radians(y).tan(),
            "max" => x.max(y),
            "min" => x.min(y),
            "mod" => (x * x + y * y + z * z).sqrt(),
            "pin" => {
                if y < x {
                    x
                } else if y > z {
                    z
                } else {
                    y
                }
            }
            "sqrt" => x.max(0.0).sqrt(),
            _ => 0.0,
        }
    }

    fn set_all(&mut self, guides: &[Guide]) {
        for g in guides {
            let v = self.eval(&g.formula);
            self.values.insert(g.name.clone(), v);
        }
    }
}

impl FormulaGeometry {
    /// The outline for a shape `w` × `h`, in the shape's coordinates.
    pub fn outline(&self, w: f64, h: f64) -> Vec<Outline> {
        let mut v = Values::new(w, h);
        v.set_all(&self.def.adjust);
        // The shape's adjust values replace the defaults of the same name.
        v.set_all(&self.adjust);
        v.set_all(&self.def.guides);
        self.def.paths.iter().map(|p| outline_path(p, &v, w, h)).collect()
    }

    /// The text rectangle for a shape `w` × `h` as left, top, right, bottom.
    pub fn text_rect(&self, w: f64, h: f64) -> (f64, f64, f64, f64) {
        if matches!(self.preset.as_deref(), Some("line" | "lineInv" | "straightConnector1")) {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let Some([l, t, r, b]) = &self.def.text_rect else { return (0.0, 0.0, w, h) };
        let mut v = Values::new(w, h);
        v.set_all(&self.def.adjust);
        v.set_all(&self.adjust);
        v.set_all(&self.def.guides);
        (v.get(l), v.get(t), v.get(r), v.get(b))
    }
}

fn outline_path(path: &GeometryPath, v: &Values, w: f64, h: f64) -> Outline {
    let (sx, sy) = if path.width > 0 && path.height > 0 {
        (w / path.width as f64, h / path.height as f64)
    } else {
        (1.0, 1.0)
    };
    let pt = |x: &str, y: &str| (v.get(x), v.get(y));
    let mut subpaths: Vec<Subpath> = Vec::new();
    let mut current = (0.0, 0.0);
    let open = |subpaths: &mut Vec<Subpath>, at: (f64, f64)| {
        if subpaths.last().is_none_or(|s| s.closed || !s.points.is_empty()) {
            subpaths.push(Subpath { points: vec![at], closed: false });
        } else if let Some(s) = subpaths.last_mut() {
            s.points.push(at);
        }
    };
    for op in &path.ops {
        match op {
            PathOp::MoveTo(x, y) => {
                current = pt(x, y);
                subpaths.push(Subpath { points: vec![current], closed: false });
            }
            PathOp::LineTo(x, y) => {
                if subpaths.last().is_none_or(|s| s.closed) {
                    open(&mut subpaths, current);
                }
                current = pt(x, y);
                subpaths.last_mut().expect("a subpath").points.push(current);
            }
            PathOp::QuadTo([cx, cy, x, y]) => {
                if subpaths.last().is_none_or(|s| s.closed) {
                    open(&mut subpaths, current);
                }
                let (c, t) = (pt(cx, cy), pt(x, y));
                let points = &mut subpaths.last_mut().expect("a subpath").points;
                for i in 1..=12 {
                    let s = i as f64 / 12.0;
                    let u = 1.0 - s;
                    points.push((u * u * current.0 + 2.0 * u * s * c.0 + s * s * t.0, u * u * current.1 + 2.0 * u * s * c.1 + s * s * t.1));
                }
                current = t;
            }
            PathOp::CubicTo([ax, ay, bx, by, x, y]) => {
                if subpaths.last().is_none_or(|s| s.closed) {
                    open(&mut subpaths, current);
                }
                let (c1, c2, t) = (pt(ax, ay), pt(bx, by), pt(x, y));
                let points = &mut subpaths.last_mut().expect("a subpath").points;
                for i in 1..=16 {
                    let s = i as f64 / 16.0;
                    let u = 1.0 - s;
                    let x = u * u * u * current.0 + 3.0 * u * u * s * c1.0 + 3.0 * u * s * s * c2.0 + s * s * s * t.0;
                    let y = u * u * u * current.1 + 3.0 * u * u * s * c1.1 + 3.0 * u * s * s * c2.1 + s * s * s * t.1;
                    points.push((x, y));
                }
                current = t;
            }
            PathOp::ArcTo { wr, hr, start, sweep } => {
                if subpaths.last().is_none_or(|s| s.closed) {
                    open(&mut subpaths, current);
                }
                let (rx, ry) = (v.get(wr), v.get(hr));
                let (start, sweep) = (v.get(start), v.get(sweep));
                let arc = arc_points(current, rx, ry, start, sweep);
                if let Some(last) = arc.last() {
                    current = *last;
                }
                subpaths.last_mut().expect("a subpath").points.extend(arc);
            }
            PathOp::Close => {
                if let Some(s) = subpaths.last_mut() {
                    s.closed = true;
                    if let Some(first) = s.points.first() {
                        current = *first;
                    }
                }
            }
        }
    }
    if (sx - 1.0).abs() > 1e-9 || (sy - 1.0).abs() > 1e-9 {
        for subpath in &mut subpaths {
            for pt in &mut subpath.points {
                pt.0 *= sx;
                pt.1 *= sy;
            }
        }
    }
    subpaths.retain(|s| s.points.len() > 1);
    Outline { subpaths, fill: path.fill, stroke: path.stroke }
}

/// Points of an `a:arcTo` from `from`. Angles are measured as on the
/// ellipse's circle before it is stretched (so 45° on a wide ellipse is
/// the point in its corner direction), in 60 000ths of a degree.
fn arc_points(from: (f64, f64), rx: f64, ry: f64, start: f64, sweep: f64) -> Vec<(f64, f64)> {
    // The parametric angle of a point at `angle` from the centre.
    let param = |angle: f64| {
        let a = radians(angle);
        (a.sin() * rx).atan2(a.cos() * ry)
    };
    let t0 = param(start);
    let center = (from.0 - rx * t0.cos(), from.1 - ry * t0.sin());
    let tau = std::f64::consts::TAU;
    // One point per 5 degrees of the ellipse's own parameter (and of the angle): a long thin ellipse turns through a few degrees of the angle
    // measured from its centre while its point goes a quarter round (a curved arrow's bend), so the steps are counted on the parameter too.
    let unwrapped = |n: usize| {
        let mut previous = t0;
        let mut length = 0.0;
        for i in 1..=n {
            let mut t = param(start + sweep * i as f64 / n as f64);
            t += ((previous - t) / tau).round() * tau;
            length += (t - previous).abs();
            previous = t;
        }
        length
    };
    let steps = ((sweep.abs() / 60_000.0).max(unwrapped(64).to_degrees()) / 5.0).ceil() as usize;
    let steps = steps.clamp(2, 128);
    let mut previous = t0;
    (1..=steps)
        .map(|i| {
            // Unwrap: the nearest turn of the angle to the previous point's.
            let mut t = param(start + sweep * i as f64 / steps as f64);
            t += ((previous - t) / tau).round() * tau;
            previous = t;
            (center.0 + rx * t.cos(), center.1 + ry * t.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide(name: &str, formula: &str) -> Guide {
        Guide { name: name.into(), formula: formula.into() }
    }

    fn s(v: &str) -> String {
        v.into()
    }

    #[test]
    fn formulas() {
        let v = Values::new(200.0, 100.0);
        assert_eq!(v.eval("*/ w 1 4"), 50.0);
        assert_eq!(v.eval("+- w h 50"), 250.0);
        assert_eq!(v.eval("+/ w h 3"), 100.0);
        assert_eq!(v.eval("?: -1 5 7"), 7.0);
        assert_eq!(v.eval("pin 0 150 100"), 100.0);
        assert_eq!(v.eval("min w h"), 100.0);
        assert!((v.eval("at2 1 1") - 2_700_000.0).abs() < 1e-6);
        assert!((v.eval("cos 100 cd4")).abs() < 1e-9);
        assert_eq!(v.eval("mod 3 4 0"), 5.0);
        assert_eq!(v.get("ssd4"), 25.0);
    }

    #[test]
    fn adjust_values_override_defaults() {
        let def = GeometryDef {
            text_rect: None,
            adjust: vec![guide("adj", "val 25000")],
            guides: vec![guide("x1", "*/ w adj 100000")],
            paths: vec![GeometryPath {
                width: 0,
                height: 0,
                fill: PathFill::Normal,
                stroke: true,
                ops: vec![PathOp::MoveTo(s("l"), s("t")), PathOp::LineTo(s("x1"), s("t")), PathOp::LineTo(s("x1"), s("b")), PathOp::Close],
            }],
        };
        let geometry = FormulaGeometry { preset: None, adjust: vec![guide("adj", "val 50000")], def: Arc::new(def) };
        let outline = geometry.outline(200.0, 100.0);
        assert_eq!(outline[0].subpaths[0].points, vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]);
        assert!(outline[0].subpaths[0].closed);
    }

    #[test]
    fn quarter_arc_ends_where_expected() {
        // From the top of a circle of radius 10 centred at (10, 10), sweep 90° clockwise.
        let points = arc_points((10.0, 0.0), 10.0, 10.0, 16_200_000.0, 5_400_000.0);
        let end = *points.last().unwrap();
        assert!((end.0 - 20.0).abs() < 1e-6 && (end.1 - 10.0).abs() < 1e-6, "{end:?}");
    }

    #[test]
    fn half_arc_on_an_ellipse() {
        // From the left end of a 20 × 10 ellipse centred at (20, 10), sweep 180°.
        let points = arc_points((0.0, 10.0), 20.0, 10.0, 10_800_000.0, 10_800_000.0);
        let end = *points.last().unwrap();
        assert!((end.0 - 40.0).abs() < 1e-6 && (end.1 - 10.0).abs() < 1e-6, "{end:?}");
        // It passes through the top of the ellipse.
        assert!(points.iter().any(|p| (p.1 - 0.0).abs() < 0.5));
    }

    #[test]
    fn scaled_path_with_arcs_preserves_continuity() {
        // A path defined in 43200x43200 local space with arcs, evaluated at 400x200 (aspect ratio 2:1).
        let def = GeometryDef {
            text_rect: None,
            adjust: Vec::new(),
            guides: Vec::new(),
            paths: vec![GeometryPath {
                width: 43200,
                height: 43200,
                fill: PathFill::Normal,
                stroke: true,
                ops: vec![
                    PathOp::MoveTo(s("3900"), s("14370")),
                    PathOp::ArcTo { wr: s("6753"), hr: s("9190"), start: s("-11429249"), sweep: s("7426832") },
                    PathOp::ArcTo { wr: s("5333"), hr: s("7267"), start: s("-8646143"), sweep: s("5396714") },
                    PathOp::Close,
                ],
            }],
        };
        let geometry = FormulaGeometry { preset: None, adjust: Vec::new(), def: Arc::new(def) };
        let outline = geometry.outline(400.0, 200.0);
        assert_eq!(outline.len(), 1);
        let subpath = &outline[0].subpaths[0];
        assert!(subpath.closed);
        let first = subpath.points.first().unwrap();
        // The start point is scaled from (3900, 14370) by (400/43200, 200/43200)
        let expected_x = 3900.0 * (400.0 / 43200.0);
        let expected_y = 14370.0 * (200.0 / 43200.0);
        assert!((first.0 - expected_x).abs() < 1e-6);
        assert!((first.1 - expected_y).abs() < 1e-6);
    }
}
