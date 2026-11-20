// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shape geometry: preset shapes and custom geometry.
//!
//! Preset shapes are defined by ECMA-376's presetShapeDefinitions.xml
//! (`data/`, used under the notice in `data/ECMA-NOTICE.txt`); a custom
//! geometry has the same structure inline.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use ibis_doc_model::{FormulaGeometry, Geometry, GeometryDef, GeometryPath, Guide, PathFill, PathOp};
use roxmltree::Node;

use crate::xml::{attr, attr_bool, attr_i64, child, children, elements};

const PRESETS_XML: &str = include_str!("../data/presetShapeDefinitions.xml");
const WARPS_XML: &str = include_str!("../data/presetTextWarpDefinitions.xml");

/// The preset text warps (`a:prstTxWarp`, ST_TextShapeType) by name, parsed on first use: each is the
/// paths text is bent to, defined like a preset shape.
fn warps() -> &'static HashMap<String, Arc<GeometryDef>> {
    static WARPS: OnceLock<HashMap<String, Arc<GeometryDef>>> = OnceLock::new();
    WARPS.get_or_init(|| {
        let text = WARPS_XML.trim_start_matches('\u{feff}');
        let Ok(doc) = roxmltree::Document::parse(text) else { return HashMap::new() };
        elements(doc.root_element()).map(|n| (n.tag_name().name().to_string(), Arc::new(definition(n)))).collect()
    })
}

/// The guide paths of text warp `name` (`"textWave1"`), with `adjust` (name and value) over its defaults;
/// `None` if there is no such warp.
pub fn text_warp(name: &str, adjust: &[(String, i64)]) -> Option<FormulaGeometry> {
    let def = warps().get(name)?.clone();
    let adjust = adjust.iter().map(|(n, v)| Guide { name: n.clone(), formula: format!("val {v}") }).collect();
    Some(FormulaGeometry { preset: Some(name.to_string()), adjust, def })
}

/// Preset definitions by name, parsed on first use.
fn presets() -> &'static HashMap<String, Arc<GeometryDef>> {
    static PRESETS: OnceLock<HashMap<String, Arc<GeometryDef>>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        let text = PRESETS_XML.trim_start_matches('\u{feff}');
        let Ok(doc) = roxmltree::Document::parse(text) else { return HashMap::new() };
        let mut map: HashMap<String, Arc<GeometryDef>> = elements(doc.root_element()).map(|n| (n.tag_name().name().to_string(), Arc::new(definition(n)))).collect();
        map.entry("upArrow".to_string()).or_insert_with(|| Arc::new(up_arrow()));
        // ECMA-376's `presetShapeDefinitions.xml` has `<rect l="il" t="ir" r="it" b="ib" />` on `pie`
        // (`t` and `r` swapped, making `l > r` and `t > b`).
        if let Some(pie) = map.get_mut("pie").and_then(Arc::get_mut) {
            pie.text_rect = Some(["il", "it", "ir", "ib"].map(str::to_string));
        }
        map
    })
}

/// `upArrow`, which the definitions file lacks (it has `upDownArrow`
/// twice where `upArrow` should be): `downArrow` pointing up, its path
/// from the head's left corner round the head first, as PowerPoint draws
/// it (where a dashed outline's pattern starts).
fn up_arrow() -> GeometryDef {
    let g = |name: &str, formula: &str| Guide { name: name.to_string(), formula: formula.to_string() };
    let pt = |x: &str, y: &str| (x.to_string(), y.to_string());
    let points = [pt("hc", "t"), pt("r", "y2"), pt("x2", "y2"), pt("x2", "b"), pt("x1", "b"), pt("x1", "y2")];
    let mut ops = vec![PathOp::MoveTo("l".to_string(), "y2".to_string())];
    ops.extend(points.into_iter().map(|(x, y)| PathOp::LineTo(x, y)));
    ops.push(PathOp::Close);
    GeometryDef {
        adjust: vec![g("adj1", "val 50000"), g("adj2", "val 50000")],
        guides: vec![
            g("maxAdj2", "*/ 100000 h ss"),
            g("a1", "pin 0 adj1 100000"),
            g("a2", "pin 0 adj2 maxAdj2"),
            g("dy2", "*/ ss a2 100000"),
            g("y2", "+- t dy2 0"),
            g("dx1", "*/ w a1 200000"),
            g("x1", "+- hc 0 dx1"),
            g("x2", "+- hc dx1 0"),
            g("dy1", "*/ x1 dy2 wd2"),
            g("y1", "+- y2 0 dy1"),
        ],
        paths: vec![GeometryPath { width: 0, height: 0, fill: PathFill::Normal, stroke: true, ops }],
        text_rect: Some(["x1", "y1", "x2", "b"].map(str::to_string)),
    }
}

fn guides(list: Option<Node>) -> Vec<Guide> {
    list.into_iter()
        .flat_map(|l| children(l, "gd"))
        .filter_map(|gd| Some(Guide { name: attr(gd, "name")?.to_string(), formula: attr(gd, "fmla")?.to_string() }))
        .collect()
}

/// Parse a geometry definition: a preset's element or `a:custGeom`.
fn definition(node: Node) -> GeometryDef {
    let point = |n: Node| -> Option<(String, String)> {
        let pt = child(n, "pt")?;
        Some((attr(pt, "x")?.to_string(), attr(pt, "y")?.to_string()))
    };
    let points = |n: Node| -> Vec<String> {
        children(n, "pt").flat_map(|pt| [attr(pt, "x").unwrap_or("0").to_string(), attr(pt, "y").unwrap_or("0").to_string()]).collect()
    };
    let paths = child(node, "pathLst")
        .into_iter()
        .flat_map(|l| children(l, "path"))
        .map(|p| {
            let ops = elements(p)
                .filter_map(|c| match c.tag_name().name() {
                    "moveTo" => point(c).map(|(x, y)| PathOp::MoveTo(x, y)),
                    "lnTo" => point(c).map(|(x, y)| PathOp::LineTo(x, y)),
                    "arcTo" => Some(PathOp::ArcTo {
                        wr: attr(c, "wR")?.to_string(),
                        hr: attr(c, "hR")?.to_string(),
                        start: attr(c, "stAng")?.to_string(),
                        sweep: attr(c, "swAng")?.to_string(),
                    }),
                    "quadBezTo" => points(c).try_into().ok().map(PathOp::QuadTo),
                    "cubicBezTo" => points(c).try_into().ok().map(PathOp::CubicTo),
                    "close" => Some(PathOp::Close),
                    _ => None,
                })
                .collect();
            GeometryPath {
                width: attr_i64(p, "w").unwrap_or(0),
                height: attr_i64(p, "h").unwrap_or(0),
                fill: match attr(p, "fill") {
                    Some("none") => PathFill::None,
                    Some("lighten") => PathFill::Lighten,
                    Some("lightenLess") => PathFill::LightenLess,
                    Some("darken") => PathFill::Darken,
                    Some("darkenLess") => PathFill::DarkenLess,
                    _ => PathFill::Normal,
                },
                stroke: attr_bool(p, "stroke").unwrap_or(true),
                ops,
            }
        })
        .collect();
    let text_rect = child(node, "rect").map(|r| ["l", "t", "r", "b"].map(|k| attr(r, k).unwrap_or("0").to_string()));
    GeometryDef { adjust: guides(child(node, "avLst")), guides: guides(child(node, "gdLst")), paths, text_rect }
}

/// The geometry of `a:prstGeom` or `a:custGeom`; a rectangle for `None`
/// or an unknown preset.
/// The geometry of preset shape `name` (`ST_ShapeType`, such as
/// `"triangle"`) with its default adjust values; `None` if there is no such preset.
pub fn preset(name: &str) -> Option<Geometry> {
    match name {
        "rect" => Some(Geometry::Rect),
        "ellipse" => Some(Geometry::Ellipse),
        "roundRect" => Some(Geometry::RoundRect { radius: 16_667 }),
        other => presets().get(other).map(|def| Geometry::Formula(FormulaGeometry { preset: Some(other.to_string()), adjust: Vec::new(), def: def.clone() })),
    }
}

pub(crate) fn geometry(node: Option<Node>) -> Geometry {
    let Some(node) = node else { return Geometry::Rect };
    if node.has_tag_name("custGeom") {
        let def = definition(node);
        if def.paths.is_empty() {
            return Geometry::Rect;
        }
        return Geometry::Formula(FormulaGeometry { preset: None, adjust: Vec::new(), def: Arc::new(def) });
    }
    let name = attr(node, "prst").unwrap_or("rect");
    let adjust = guides(child(node, "avLst"));
    match name {
        "rect" => Geometry::Rect,
        "ellipse" => Geometry::Ellipse,
        "roundRect" => {
            // The corner radius, as the fraction of the shorter side: adj / 100 000.
            let adj = adjust
                .iter()
                .find(|g| g.name == "adj")
                .and_then(|g| g.formula.strip_prefix("val "))
                .and_then(|v| v.trim().parse::<i64>().ok())
                .unwrap_or(16_667)
                .clamp(0, 50_000);
            Geometry::RoundRect { radius: adj as u32 }
        }
        other => match presets().get(other) {
            Some(def) => Geometry::Formula(FormulaGeometry { preset: Some(other.to_string()), adjust, def: def.clone() }),
            None => Geometry::Rect,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 41 text warps parse, each with its guide paths; a wave's two curves run across the box.
    #[test]
    fn all_text_warps_parse() {
        assert!(warps().len() >= 40, "{} warps", warps().len());
        for (name, def) in warps() {
            assert!(!def.paths.is_empty(), "{name} has no path");
        }
        // The warp's own adjust value changes its curves.
        let plain = text_warp("textWave1", &[]).unwrap().outline(1000.0, 500.0);
        let strong = text_warp("textWave1", &[("adj1".to_string(), 40_000)]).unwrap().outline(1000.0, 500.0);
        assert_ne!(plain, strong);
        let wave = text_warp("textWave1", &[]).unwrap();
        let outlines = wave.outline(1000.0, 500.0);
        assert_eq!(outlines.len(), 2);
        for o in &outlines {
            let xs: Vec<f64> = o.subpaths.iter().flat_map(|s| s.points.iter().map(|p| p.0)).collect();
            assert!(xs.iter().cloned().fold(f64::MAX, f64::min) < 1.0 && xs.iter().cloned().fold(f64::MIN, f64::max) > 999.0);
        }
    }

    #[test]
    fn all_presets_parse() {
        let presets = presets();
        assert!(presets.len() > 180, "{} presets", presets.len());
        for (name, def) in presets {
            assert!(!def.paths.is_empty(), "{name} has no path");
        }
    }

    #[test]
    fn triangle_outline() {
        let def = presets()["triangle"].clone();
        let g = FormulaGeometry { preset: Some("triangle".into()), adjust: Vec::new(), def };
        let outline = g.outline(100.0, 80.0);
        assert_eq!(outline[0].subpaths[0].points, vec![(0.0, 80.0), (50.0, 0.0), (100.0, 80.0)]);
    }

    #[test]
    fn bent_up_arrow_has_its_corners() {
        let def = presets()["bentUpArrow"].clone();
        let g = FormulaGeometry { preset: None, adjust: Vec::new(), def };
        let points = &g.outline(100.0, 100.0)[0].subpaths[0].points;
        // The arrow's tip is at the top of the shape, in its right part.
        assert!(points.iter().any(|&(x, y)| y == 0.0 && x > 50.0), "{points:?}");
        assert!(points.len() >= 9);
    }
}

