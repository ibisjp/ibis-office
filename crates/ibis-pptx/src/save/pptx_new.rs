// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! New PresentationML for things the model has and the file does not: a
//! shape put on a slide, a slide made from a layout, the fill or outline
//! written into a shape. The presentation担当's file (see `pptx.rs`).
//!
//! Elements are written as text with the prefixes the target part uses,
//! parsed, and imported into the part's tree
//! ([`Fragment::import_into`]), so what is new reads as if PowerPoint had
//! written it and nothing else of the part changes.

use std::fmt::Write as _;

use ibis_doc_model::{
    Color, Fill, FormulaGeometry, Geometry, LineEnd, LineSpacing, Paragraph, PathFill, PathOp, Picture, Run, Shape, ShapeKind, Stroke, TextAlign, TextBody,
    TextStyle, VerticalAnchor,
};
use ibis_ooxml::{NodeId, XmlDocument};

use super::unsupported;
use crate::Error;

pub(crate) const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub(crate) const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
pub(crate) const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(crate) const NS_XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";

/// The prefixes (with their colons) to name elements of the DrawingML,
/// PresentationML and relationship namespaces with at a place of a part,
/// and the declarations a new element needs for those not bound there.
#[derive(Clone, Debug)]
pub(crate) struct Prefixes {
    pub a: String,
    pub p: String,
    pub r: String,
    /// `(prefix, namespace)` to declare on the new element.
    missing: Vec<(String, &'static str)>,
    /// The namespace `p` stands for: PresentationML, or (in a sheet's drawing) SpreadsheetDrawing.
    p_ns: &'static str,
}

impl Prefixes {
    pub(crate) fn at(doc: &XmlDocument, node: NodeId) -> Self {
        let mut missing = Vec::new();
        let mut get = |ns: &'static str, fallback: &str| match doc.lookup_prefix(node, ns) {
            Some(p) if p.is_empty() => String::new(),
            Some(p) => format!("{p}:"),
            None => {
                missing.push((fallback.to_string(), ns));
                format!("{fallback}:")
            }
        };
        let a = get(NS_A, "a");
        let p = get(NS_P, "p");
        let r = get(NS_R, "r");
        Self { a, p, r, missing, p_ns: NS_P }
    }

    /// The prefixes at a place of a sheet's drawing part (`xdr:wsDr`), where `p` names the
    /// SpreadsheetDrawing namespace the shapes (`xdr:sp`, `xdr:pic`) are in.
    pub(crate) fn drawing(doc: &XmlDocument, node: NodeId) -> Self {
        let mut missing = Vec::new();
        let mut get = |ns: &'static str, fallback: &str| match doc.lookup_prefix(node, ns) {
            Some(p) if p.is_empty() => String::new(),
            Some(p) => format!("{p}:"),
            None => {
                missing.push((fallback.to_string(), ns));
                format!("{fallback}:")
            }
        };
        let a = get(NS_A, "a");
        let p = get(NS_XDR, "xdr");
        let r = get(NS_R, "r");
        Self { a, p, r, missing, p_ns: NS_XDR }
    }

    /// Parse `xml` (one element written with these prefixes) and import it
    /// into `doc`, detached, with declarations for the prefixes `doc` does
    /// not bind where it goes.
    pub(crate) fn import(&self, doc: &mut XmlDocument, xml: &str) -> Result<NodeId, Error> {
        let mut wrapper = String::from("<w");
        for (prefix, ns) in [(&self.a, NS_A), (&self.p, self.p_ns), (&self.r, NS_R)] {
            match prefix.strip_suffix(':') {
                Some(p) => write!(wrapper, " xmlns:{p}=\"{ns}\"").unwrap(),
                None => write!(wrapper, " xmlns=\"{ns}\"").unwrap(),
            }
        }
        write!(wrapper, ">{xml}</w>").unwrap();
        let parsed = XmlDocument::parse(wrapper.as_bytes()).map_err(|e| Error::new(format!("新しい要素を作れません: {e}")))?;
        let root = parsed.root_element().ok_or_else(|| Error::new("新しい要素を作れません"))?;
        let element = parsed.elements(root).next().ok_or_else(|| Error::new("新しい要素を作れません"))?;
        let id = doc.import(&parsed, element);
        for (prefix, ns) in &self.missing {
            doc.set_attr(id, &format!("xmlns:{prefix}"), ns);
        }
        Ok(id)
    }
}

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Characters XML 1.0 cannot hold are left out.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn escape_attr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            // Whitespace other than space is normalized by XML parsers unless escaped as character references.
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

/// `a:srgbClr` for `color`, with its opacity.
pub(crate) fn color_xml(x: &Prefixes, color: Color) -> String {
    let a = &x.a;
    let rgb = format!("{:06X}", color.rgb.0 & 0xff_ffff);
    if color.alpha == 255 {
        format!("<{a}srgbClr val=\"{rgb}\"/>")
    } else {
        let alpha = (color.alpha as f64 / 255.0 * 100_000.0).round() as i64;
        format!("<{a}srgbClr val=\"{rgb}\"><{a}alpha val=\"{alpha}\"/></{a}srgbClr>")
    }
}

/// Validate values shared by shape, background and cell fill writers.
pub(crate) fn check_fill(fill: &Fill) -> Result<(), Error> {
    let stops=match fill {
        Fill::Linear {angle,stops,..} | Fill::TileLinear {angle,stops,..} => {
            if !(0..21_600_000).contains(angle) {return Err(unsupported("グラデーション角度の範囲"));}
            stops
        },
        Fill::Path {stops,..} => stops,
        Fill::Pattern {preset,..} => {
            if ibis_doc_model::pattern_tile(preset).is_none() {return Err(unsupported("このパターンの種類"));}
            return Ok(());
        },
        _ => return Ok(()),
    };
    if stops.len()<2 || stops.iter().any(|s|s.position>100_000) {return Err(unsupported("グラデーションstopの数・位置の範囲"));}
    Ok(())
}

/// The fill element for `fill`. Picture fills are not written yet.
pub(crate) fn fill_xml(x: &Prefixes, fill: &Fill) -> Result<String, Error> {
    check_fill(fill)?;
    let a = &x.a;
    Ok(match fill {
        Fill::None => format!("<{a}noFill/>"),
        Fill::Solid(color) => format!("<{a}solidFill>{}</{a}solidFill>", color_xml(x, *color)),
        Fill::Linear { angle, scaled, stops } => {
            let mut s = format!("<{a}gradFill rotWithShape=\"1\"><{a}gsLst>");
            for stop in stops {
                write!(s, "<{a}gs pos=\"{}\">{}</{a}gs>", stop.position, color_xml(x, stop.color)).unwrap();
            }
            let sc = if *scaled { "1" } else { "0" };
            write!(s, "</{a}gsLst><{a}lin ang=\"{angle}\" scaled=\"{sc}\"/></{a}gradFill>").unwrap();
            s
        }
        Fill::TileLinear { angle, scaled, stops, tile, flip } => {
            let flip_str = match flip {
                ibis_doc_model::TileFlip::X => " flip=\"x\"",
                ibis_doc_model::TileFlip::Y => " flip=\"y\"",
                ibis_doc_model::TileFlip::Xy => " flip=\"xy\"",
                _ => "",
            };
            let mut s = format!("<{a}gradFill{flip_str} rotWithShape=\"1\"><{a}gsLst>");
            for stop in stops {
                write!(s, "<{a}gs pos=\"{}\">{}</{a}gs>", stop.position, color_xml(x, stop.color)).unwrap();
            }
            let sc = if *scaled { "1" } else { "0" };
            let [l, t, r, b] = tile;
            write!(s, "</{a}gsLst><{a}lin ang=\"{angle}\" scaled=\"{sc}\"/><{a}tileRect l=\"{l}\" t=\"{t}\" r=\"{r}\" b=\"{b}\"/></{a}gradFill>").unwrap();
            s
        }
        Fill::Path { path, focus, stops } => {
            let mut s = format!("<{a}gradFill rotWithShape=\"1\"><{a}gsLst>");
            for stop in stops {
                write!(s, "<{a}gs pos=\"{}\">{}</{a}gs>", stop.position, color_xml(x, stop.color)).unwrap();
            }
            let [l, t, r, b] = focus;
            write!(s, "</{a}gsLst><{a}path path=\"{}\"><{a}fillToRect l=\"{l}\" t=\"{t}\" r=\"{r}\" b=\"{b}\"/></{a}path></{a}gradFill>", path.name()).unwrap();
            s
        }
        Fill::Image(media) => {
            let rel = MEDIA_RELS.with(|m| m.borrow().get(media).cloned()).ok_or_else(|| unsupported("画像塗りつぶしの関係ID"))?;
            let blip_ext = if is_svg_media(*media) {
                format!("<{a}extLst><{a}ext uri=\"{{96DAC542-7B81-4868-9F9E-1EA1F31DC1F3}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" {r}embed=\"{rel}\"/></{a}ext></{a}extLst>", a = x.a, r = x.r)
            } else {
                String::new()
            };
            format!("<{a}blipFill><{a}blip {r}embed=\"{rel}\">{blip_ext}</{a}blip><{a}stretch><{a}fillRect/></{a}stretch></{a}blipFill>", r = x.r)
        }
        Fill::TileImage { media, tx, ty, sx, sy, flip, align } => {
            let rel = MEDIA_RELS.with(|m| m.borrow().get(media).cloned()).ok_or_else(|| unsupported("画像塗りつぶしの関係ID"))?;
            let blip_ext = if is_svg_media(*media) {
                format!("<{a}extLst><{a}ext uri=\"{{96DAC542-7B81-4868-9F9E-1EA1F31DC1F3}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" {r}embed=\"{rel}\"/></{a}ext></{a}extLst>", a = x.a, r = x.r)
            } else {
                String::new()
            };
            let flip_attr = match flip {
                ibis_doc_model::TileFlip::None => "none",
                ibis_doc_model::TileFlip::X => "x",
                ibis_doc_model::TileFlip::Y => "y",
                ibis_doc_model::TileFlip::Xy => "xy",
            };
            let algn_attr = match align {
                ibis_doc_model::TileAlign::TopLeft => "tl",
                ibis_doc_model::TileAlign::Top => "t",
                ibis_doc_model::TileAlign::TopRight => "tr",
                ibis_doc_model::TileAlign::Left => "l",
                ibis_doc_model::TileAlign::Center => "ctr",
                ibis_doc_model::TileAlign::Right => "r",
                ibis_doc_model::TileAlign::BottomLeft => "bl",
                ibis_doc_model::TileAlign::Bottom => "b",
                ibis_doc_model::TileAlign::BottomRight => "br",
            };
            format!("<{a}blipFill><{a}blip {r}embed=\"{rel}\">{blip_ext}</{a}blip><{a}tile tx=\"{tx}\" ty=\"{ty}\" sx=\"{sx}\" sy=\"{sy}\" flip=\"{flip_attr}\" algn=\"{algn_attr}\"/></{a}blipFill>", r = x.r)
        }
        Fill::Blip(blip) => {
            let media = blip.media;
            let rel = MEDIA_RELS.with(|m| m.borrow().get(&media).cloned()).ok_or_else(|| unsupported("画像塗りつぶしの関係ID"))?;
            let mut effects = String::new();
            let e = &blip.effects;
            if let Some(v) = e.opacity {
                write!(effects, "<{a}alphaModFix amt=\"{v}\"/>").unwrap();
            }
            if e.grayscale {
                write!(effects, "<{a}grayscl/>").unwrap();
            }
            if let Some(v) = e.bilevel {
                write!(effects, "<{a}biLevel thresh=\"{v}\"/>").unwrap();
            }
            if let Some((bright, contrast)) = blip.luminance {
                write!(effects, "<{a}lum bright=\"{bright}\" contrast=\"{contrast}\"/>").unwrap();
            }
            if let Some((dark, light)) = blip.duotone {
                write!(effects, "<{a}duotone><{a}srgbClr val=\"{:06X}\"/><{a}srgbClr val=\"{:06X}\"/></{a}duotone>", dark.0, light.0).unwrap();
            }
            let blip_ext = if is_svg_media(media) {
                format!("<{a}extLst><{a}ext uri=\"{{96DAC542-7B81-4868-9F9E-1EA1F31DC1F3}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" {r}embed=\"{rel}\"/></{a}ext></{a}extLst>", r = x.r)
            } else {
                String::new()
            };
            let crop = &blip.crop;
            let src = if *crop == ibis_doc_model::Crop::default() {
                String::new()
            } else {
                format!("<{a}srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>", crop.left, crop.top, crop.right, crop.bottom)
            };
            let place = match (blip.tile, blip.fill_rect) {
                (Some(t), _) => {
                    let flip = match t.flip {
                        ibis_doc_model::TileFlip::None => "none",
                        ibis_doc_model::TileFlip::X => "x",
                        ibis_doc_model::TileFlip::Y => "y",
                        ibis_doc_model::TileFlip::Xy => "xy",
                    };
                    let algn = match t.align {
                        ibis_doc_model::TileAlign::TopLeft => "tl",
                        ibis_doc_model::TileAlign::Top => "t",
                        ibis_doc_model::TileAlign::TopRight => "tr",
                        ibis_doc_model::TileAlign::Left => "l",
                        ibis_doc_model::TileAlign::Center => "ctr",
                        ibis_doc_model::TileAlign::Right => "r",
                        ibis_doc_model::TileAlign::BottomLeft => "bl",
                        ibis_doc_model::TileAlign::Bottom => "b",
                        ibis_doc_model::TileAlign::BottomRight => "br",
                    };
                    format!("<{a}tile tx=\"{}\" ty=\"{}\" sx=\"{}\" sy=\"{}\" flip=\"{flip}\" algn=\"{algn}\"/>", t.tx, t.ty, t.sx, t.sy)
                }
                (None, Some([l, t, r, b])) => format!("<{a}stretch><{a}fillRect l=\"{l}\" t=\"{t}\" r=\"{r}\" b=\"{b}\"/></{a}stretch>"),
                (None, None) => format!("<{a}stretch><{a}fillRect/></{a}stretch>"),
            };
            format!("<{a}blipFill><{a}blip {r}embed=\"{rel}\">{effects}{blip_ext}</{a}blip>{src}{place}</{a}blipFill>", r = x.r)
        }
        Fill::Pattern { preset, foreground, background } => format!(
            "<{a}pattFill prst=\"{}\"><{a}fgClr>{}</{a}fgClr><{a}bgClr>{}</{a}bgClr></{a}pattFill>",
            escape(preset),
            color_xml(x, *foreground),
            color_xml(x, *background)
        ),
    })
}

fn end_xml(x: &Prefixes, name: &str, end: LineEnd) -> String {
    let (kind, size) = match end {
        LineEnd::None => return String::new(),
        LineEnd::Triangle(s) => ("triangle", s),
        LineEnd::Stealth(s) => ("stealth", s),
        LineEnd::Arrow(s) => ("arrow", s),
        LineEnd::Diamond(s) => ("diamond", s),
        LineEnd::Oval(s) => ("oval", s),
    };
    let size_name = |v: u8| match v {
        0 => "sm",
        2 => "lg",
        _ => "med",
    };
    format!("<{}{name} type=\"{kind}\" w=\"{}\" len=\"{}\"/>", x.a, size_name(size.width), size_name(size.length))
}

/// `a:ln` for an outline (`None`: an explicit "no line").
pub(crate) fn line_xml(x: &Prefixes, stroke: Option<&Stroke>) -> String {
    let a = &x.a;
    match stroke {
        None => format!("<{a}ln><{a}noFill/></{a}ln>"),
        Some(s) => {
            let compound = if s.compound == ibis_doc_model::LineCompound::Single { String::new() } else { format!(" cmpd=\"{}\"", s.compound.name()) };
            let alignment = if s.alignment == ibis_doc_model::LineAlignment::Center { String::new() } else { format!(" algn=\"{}\"", s.alignment.name()) };
            let fill_str = match &s.fill {
                Some(fill) => fill_xml(x, fill).unwrap_or_else(|_| format!("<{a}solidFill>{}</{a}solidFill>", color_xml(x, s.color))),
                None => format!("<{a}solidFill>{}</{a}solidFill>", color_xml(x, s.color)),
            };
            let mut out = format!("<{a}ln w=\"{}\" cap=\"{}\"{compound}{alignment}>{fill_str}", s.width, cap_name(s.cap));
            if let Some(dash) = dash_name(s) {
                write!(out, "<{a}prstDash val=\"{dash}\"/>").unwrap();
            }
            if let Some(join) = join_element(x, s.join) {
                out.push_str(&join);
            }
            out.push_str(&end_xml(x, "headEnd", s.head));
            out.push_str(&end_xml(x, "tailEnd", s.tail));
            write!(out, "</{a}ln>").unwrap();
            out
        }
    }
}

/// `a:round` / `a:bevel` for a line join (`None` for default miter).
pub(crate) fn join_element(x: &Prefixes, join: ibis_doc_model::LineJoin) -> Option<String> {
    let a = &x.a;
    match join {
        ibis_doc_model::LineJoin::Round => Some(format!("<{a}round/>")),
        ibis_doc_model::LineJoin::Bevel => Some(format!("<{a}bevel/>")),
        ibis_doc_model::LineJoin::Miter => None,
    }
}

/// `a:ln/@cap` for a cap.
pub(crate) fn cap_name(cap: ibis_doc_model::LineCap) -> &'static str {
    match cap {
        ibis_doc_model::LineCap::Flat => "flat",
        ibis_doc_model::LineCap::Round => "rnd",
        ibis_doc_model::LineCap::Square => "sq",
    }
}

/// `a:prstDash/@val` for a stroke, `None` for a solid one.
pub(crate) fn dash_name(s: &Stroke) -> Option<&'static str> {
    match s.dash {
        ibis_doc_model::LineDash::Solid if s.dashed => Some("dash"),
        ibis_doc_model::LineDash::Solid => None,
        dash => Some(dash.name()),
    }
}

/// `a:headEnd` / `a:tailEnd` for writing into an existing `a:ln`
/// (`None` for no decoration).
pub(crate) fn end_element(x: &Prefixes, name: &str, end: LineEnd) -> Option<String> {
    let s = end_xml(x, name, end);
    (!s.is_empty()).then_some(s)
}

/// `a:xfrm` for a shape's frame, rotation and flips.
pub(crate) fn xfrm_xml(x: &Prefixes, element: &str, shape: &Shape) -> String {
    let mut attrs = String::new();
    if shape.rotation != 0 {
        write!(attrs, " rot=\"{}\"", shape.rotation).unwrap();
    }
    if shape.flip_h {
        attrs.push_str(" flipH=\"1\"");
    }
    if shape.flip_v {
        attrs.push_str(" flipV=\"1\"");
    }
    let f = shape.frame;
    let a = &x.a;
    format!("<{element}{attrs}><{a}off x=\"{}\" y=\"{}\"/><{a}ext cx=\"{}\" cy=\"{}\"/></{element}>", f.x, f.y, f.width, f.height)
}

/// `a:prstGeom` or `a:custGeom` for `geometry`.
pub(crate) fn geometry_xml(x: &Prefixes, geometry: &Geometry) -> Result<String, Error> {
    let a = &x.a;
    Ok(match geometry {
        Geometry::Rect => format!("<{a}prstGeom prst=\"rect\"><{a}avLst/></{a}prstGeom>"),
        Geometry::Ellipse => format!("<{a}prstGeom prst=\"ellipse\"><{a}avLst/></{a}prstGeom>"),
        Geometry::RoundRect { radius } => {
            format!("<{a}prstGeom prst=\"roundRect\"><{a}avLst><{a}gd name=\"adj\" fmla=\"val {radius}\"/></{a}avLst></{a}prstGeom>")
        }
        Geometry::Formula(f) => {
            if let Some(preset) = &f.preset {
                let mut s = format!("<{a}prstGeom prst=\"{}\"><{a}avLst>", escape(preset));
                for g in &f.adjust {
                    write!(s, "<{a}gd name=\"{}\" fmla=\"{}\"/>", escape(&g.name), escape(&g.formula)).unwrap();
                }
                write!(s, "</{a}avLst></{a}prstGeom>").unwrap();
                s
            } else {
                cust_geom_xml(x, f)?
            }
        }
    })
}

/// Serialize custom geometry as DrawingML `<a:custGeom>`.
pub(crate) fn cust_geom_xml(x: &Prefixes, f: &FormulaGeometry) -> Result<String, Error> {
    let a = &x.a;
    let mut s = format!("<{a}custGeom>");
    // 1. avLst
    s.push_str(&format!("<{a}avLst>"));
    for g in &f.adjust {
        write!(s, "<{a}gd name=\"{}\" fmla=\"{}\"/>", escape(&g.name), escape(&g.formula)).unwrap();
    }
    s.push_str(&format!("</{a}avLst>"));

    // 2. gdLst
    s.push_str(&format!("<{a}gdLst>"));
    for g in &f.def.guides {
        write!(s, "<{a}gd name=\"{}\" fmla=\"{}\"/>", escape(&g.name), escape(&g.formula)).unwrap();
    }
    s.push_str(&format!("</{a}gdLst>"));

    // 3. ahLst
    s.push_str(&format!("<{a}ahLst/>"));

    // 4. cxnLst
    s.push_str(&format!("<{a}cxnLst/>"));

    // 5. rect (text rectangle)
    if let Some([l, t, r, b]) = &f.def.text_rect {
        write!(s, "<{a}rect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>", escape(l), escape(t), escape(r), escape(b)).unwrap();
    }

    // 6. pathLst
    s.push_str(&format!("<{a}pathLst>"));
    for path in &f.def.paths {
        let fill_attr = match path.fill {
            PathFill::Normal => "",
            PathFill::None => " fill=\"none\"",
            PathFill::Lighten => " fill=\"lighten\"",
            PathFill::LightenLess => " fill=\"lightenLess\"",
            PathFill::Darken => " fill=\"darken\"",
            PathFill::DarkenLess => " fill=\"darkenLess\"",
        };
        let stroke_attr = if !path.stroke { " stroke=\"0\"" } else { "" };
        let mut path_tag = format!("<{a}path");
        if path.width > 0 {
            write!(path_tag, " w=\"{}\"", path.width).unwrap();
        }
        if path.height > 0 {
            write!(path_tag, " h=\"{}\"", path.height).unwrap();
        }
        path_tag.push_str(fill_attr);
        path_tag.push_str(stroke_attr);
        path_tag.push('>');
        s.push_str(&path_tag);

        for op in &path.ops {
            match op {
                PathOp::MoveTo(x_val, y_val) => {
                    write!(s, "<{a}moveTo><{a}pt x=\"{}\" y=\"{}\"/></{a}moveTo>", escape(x_val), escape(y_val)).unwrap();
                }
                PathOp::LineTo(x_val, y_val) => {
                    write!(s, "<{a}lnTo><{a}pt x=\"{}\" y=\"{}\"/></{a}lnTo>", escape(x_val), escape(y_val)).unwrap();
                }
                PathOp::ArcTo { wr, hr, start, sweep } => {
                    write!(s, "<{a}arcTo wR=\"{}\" hR=\"{}\" stAng=\"{}\" swAng=\"{}\"/>", escape(wr), escape(hr), escape(start), escape(sweep)).unwrap();
                }
                PathOp::QuadTo([cx, cy, x_val, y_val]) => {
                    write!(
                        s,
                        "<{a}quadBezTo><{a}pt x=\"{}\" y=\"{}\"/><{a}pt x=\"{}\" y=\"{}\"/></{a}quadBezTo>",
                        escape(cx), escape(cy), escape(x_val), escape(y_val)
                    ).unwrap();
                }
                PathOp::CubicTo([ax, ay, bx, by, x_val, y_val]) => {
                    write!(
                        s,
                        "<{a}cubicBezTo><{a}pt x=\"{}\" y=\"{}\"/><{a}pt x=\"{}\" y=\"{}\"/><{a}pt x=\"{}\" y=\"{}\"/></{a}cubicBezTo>",
                        escape(ax), escape(ay), escape(bx), escape(by), escape(x_val), escape(y_val)
                    ).unwrap();
                }
                PathOp::Close => {
                    write!(s, "<{a}close/>").unwrap();
                }
            }
        }
        s.push_str(&format!("</{a}path>"));
    }
    s.push_str(&format!("</{a}pathLst>"));
    s.push_str(&format!("</{a}custGeom>"));
    Ok(s)
}

fn align_value(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "l",
        TextAlign::Center => "ctr",
        TextAlign::Right => "r",
        TextAlign::Justify => "just",
        TextAlign::Distributed => "dist",
    }
}

/// `a:rPr` (or `a:endParaRPr`, by `element`) with every character format
/// of a run written out, so it reads back the same wherever it is.
fn run_props_xml(x: &Prefixes, element: &str, body: &TextStyle, run: &ibis_doc_model::RunStyle, text: &str) -> String {
    let a = &x.a;
    let style = body.with_run(run);
    let lang_attr = match &run.lang {
        Some(l) => format!(" lang=\"{l}\""),
        None => String::new(),
    };
    let mut attrs = format!("{lang_attr} altLang=\"en-US\" sz=\"{}\" b=\"{}\" i=\"{}\"", style.font_size, style.bold as u8, style.italic as u8);
    if run.underline {
        write!(attrs, " u=\"{}\"", run.underline_kind.value()).unwrap();
    }
    if run.baseline != 0 {
        write!(attrs, " baseline=\"{}\"", run.baseline).unwrap();
    }
    if run.spacing != 0 {
        write!(attrs, " spc=\"{}\"", run.spacing).unwrap();
    }
    attrs.push_str(" dirty=\"0\"");
    let mut children = String::new();
    if let Some((rgb, width)) = run.outline {
        write!(children, "<{a}ln w=\"{width}\"><{a}solidFill>{}</{a}solidFill></{a}ln>", color_xml(x, Color::opaque(rgb))).unwrap();
    }
    write!(children, "<{a}solidFill>{}</{a}solidFill>", color_xml(x, Color::opaque(style.color))).unwrap();
    if let Some(sh) = &run.shadow {
        write!(
            children,
            "<{a}effectLst><{a}outerShdw blurRad=\"{}\" dist=\"{}\" dir=\"{}\" algn=\"{}\">{}</{a}outerShdw></{a}effectLst>",
            sh.blur, sh.distance, sh.direction, sh.align.name(),
            color_xml(x, sh.color),
        ).unwrap();
    }
    if let Some(rgb) = run.highlight {
        write!(children, "<{a}highlight>{}</{a}highlight>", color_xml(x, Color::opaque(rgb))).unwrap();
    }
    children.push_str(&typefaces_xml(x, style.font.as_deref(), style.fallback_font.as_deref(), text));
    if let Some(link) = run.link.as_ref().and_then(|l| super::pptx_links::hlink_xml(x, l, run.link_tooltip.as_deref())) {
        children.push_str(&link);
    }
    format!("<{a}{element}{attrs}>{children}</{a}{element}>")
}

/// `a:latin` and `a:ea` for the typeface of a run with text `text` (the
/// one of its script) and its other script's, or the theme's body fonts
/// (`+mn-lt`, …) for none, as PowerPoint writes new text.
pub(crate) fn typefaces_xml(x: &Prefixes, font: Option<&str>, fallback: Option<&str>, text: &str) -> String {
    let a = &x.a;
    let (latin, ea) = typefaces(font, fallback, text);
    let mut out = String::new();
    for (name, face) in [("latin", latin), ("ea", ea)] {
        if let Some(face) = face {
            out.push_str(&format!("<{a}{name} typeface=\"{}\"/>", escape(&face)));
        }
    }
    if font.is_none() {
        out.push_str(&format!("<{a}cs typeface=\"+mn-cs\"/>"));
    }
    out
}

/// The Latin and East Asian typefaces to write for a run's font and its
/// other script's font (theme references for none).
pub(crate) fn typefaces(font: Option<&str>, fallback: Option<&str>, text: &str) -> (Option<String>, Option<String>) {
    match font {
        None => (Some("+mn-lt".into()), Some("+mn-ea".into())),
        Some(font) if text.chars().any(ibis_doc_model::is_east_asian) => (fallback.map(str::to_string), Some(font.to_string())),
        Some(font) => (Some(font.to_string()), fallback.map(str::to_string)),
    }
}

/// A field's `type` (`slidenum`, `datetime3`, …).
pub(crate) fn field_type(field: ibis_doc_model::TextField) -> String {
    match field {
        ibis_doc_model::TextField::SlideNumber => "slidenum".into(),
        ibis_doc_model::TextField::DateTime { format: 0, .. } => "datetime".into(),
        ibis_doc_model::TextField::DateTime { format: 14, .. } => "datetimeyyyy".into(),
        ibis_doc_model::TextField::DateTime { format, .. } => format!("datetime{format}"),
    }
}

/// `rpr` (an `a:rPr` written) with the language a date field reads its
/// form by (Japanese for an East Asian one, else U.S. English), unless it
/// has one.
fn with_field_lang(rpr: String, a: &str, field: ibis_doc_model::TextField) -> String {
    let ibis_doc_model::TextField::DateTime { east_asian, .. } = field else { return rpr };
    let lang = if east_asian { "ja-JP" } else { "en-US" };
    if rpr.contains(" lang=") {
        return rpr;
    }
    let open = format!("<{a}rPr");
    match rpr.strip_prefix(&open) {
        Some(rest) => format!("{open} lang=\"{lang}\"{rest}"),
        None => format!("<{a}rPr lang=\"{lang}\"/>{rpr}"),
    }
}

fn run_xml(x: &Prefixes, body: &TextStyle, run: &Run, seed: u64, field: &mut u32) -> String {
    let a = &x.a;
    if let Some(kind) = run.style.field {
        *field += 1;
        // Stable per model shape and field ordinal: repeated saves are identical.
        let id = format!("{{{:08X}-{:04X}-4000-8000-{:04X}{:08X}}}", seed >> 32, (seed >> 16) & 0xffff, seed & 0xffff, field);
        let rpr = with_field_lang(run_props_xml(x, "rPr", body, &run.style, &run.text), a, kind);
        return format!("<{a}fld id=\"{id}\" type=\"{}\">{rpr}<{a}t>{}</{a}t></{a}fld>", field_type(kind), escape(&run.text));
    }
    format!("<{a}r>{}<{a}t>{}</{a}t></{a}r>", run_props_xml(x, "rPr", body, &run.style, &run.text), escape(&run.text))
}

fn paragraph_xml(x: &Prefixes, body: &TextStyle, p: &Paragraph, seed: u64, field: &mut u32) -> String {
    let a = &x.a;
    let mut attrs = String::new();
    if p.level != 0 {
        write!(attrs, " lvl=\"{}\"", p.level).unwrap();
    }
    if p.margin_left != 0 {
        write!(attrs, " marL=\"{}\"", p.margin_left).unwrap();
    }
    if p.indent != 0 {
        write!(attrs, " indent=\"{}\"", p.indent).unwrap();
    }
    write!(attrs, " algn=\"{}\"", align_value(p.align.unwrap_or(body.align))).unwrap();
    let font_align = match p.font_align { ibis_doc_model::FontAlign::Auto => "auto", ibis_doc_model::FontAlign::Baseline => "base", ibis_doc_model::FontAlign::Top => "t", ibis_doc_model::FontAlign::Center => "ctr", ibis_doc_model::FontAlign::Bottom => "b" };
    write!(attrs, " fontAlgn=\"{font_align}\"").unwrap();
    let mut children = String::new();
    match p.line_spacing {
        Some(LineSpacing::Percent(v)) => write!(children, "<{a}lnSpc><{a}spcPct val=\"{v}\"/></{a}lnSpc>").unwrap(),
        Some(LineSpacing::Points(v)) => write!(children, "<{a}lnSpc><{a}spcPts val=\"{v}\"/></{a}lnSpc>").unwrap(),
        None => {}
    }
    if p.space_before != 0 {
        write!(children, "<{a}spcBef><{a}spcPts val=\"{}\"/></{a}spcBef>", p.space_before).unwrap();
    }
    if p.space_after != 0 {
        write!(children, "<{a}spcAft><{a}spcPts val=\"{}\"/></{a}spcAft>", p.space_after).unwrap();
    }
    match &p.bullet {
        Some(bullet) if bullet.picture.is_none() && !bullet.text.is_empty() => {
            if let Some(rgb) = bullet.color {
                write!(children, "<{a}buClr>{}</{a}buClr>", color_xml(x, Color::opaque(rgb))).unwrap();
            }
            if bullet.size != 100_000 {
                write!(children, "<{a}buSzPct val=\"{}\"/>", bullet.size).unwrap();
            }
            if let Some(auto) = &bullet.auto {
                let start = if auto.start_at > 1 { format!(" startAt=\"{}\"", auto.start_at) } else { String::new() };
                write!(children, "<{a}buFont typeface=\"+mj-lt\"/><{a}buAutoNum type=\"{}\"{start}/>", escape(&auto.scheme)).unwrap();
            } else {
                let c: String = bullet.text.chars().take(1).collect();
                write!(children, "<{a}buFont typeface=\"Arial\"/><{a}buChar char=\"{}\"/>", escape(&c)).unwrap();
            }
        }
        Some(_) => {}
        None => write!(children, "<{a}buNone/>").unwrap(),
    }
    if !p.tabs.is_empty() {
        write!(children, "<{a}tabLst>").unwrap();
        for tab in &p.tabs {
            let algn = match tab.align {
                ibis_doc_model::TextTabAlign::Center => " algn=\"c\"",
                ibis_doc_model::TextTabAlign::Right => " algn=\"r\"",
                ibis_doc_model::TextTabAlign::Decimal => " algn=\"dec\"",
                ibis_doc_model::TextTabAlign::Left => "",
            };
            write!(children, "<{a}tab pos=\"{}\"{algn}/>", tab.position).unwrap();
        }
        write!(children, "</{a}tabLst>").unwrap();
    }
    let mut s = format!("<{a}p><{a}pPr{attrs}>{children}</{a}pPr>");
    for run in &p.runs {
        match &run.style.math {
            Some(math) => s.push_str(&crate::math_xml::write_math_content(math, a, run.style.math_inline)),
            None => s.push_str(&run_xml(x, body, run, seed, field)),
        }
    }
    s.push_str(&run_props_xml(x, "endParaRPr", body, &p.end_style, ""));
    write!(s, "</{a}p>").unwrap();
    s
}

/// `p:txBody` for `body`. `anchor_center` is for a shape with no text,
/// whose text PowerPoint centres.
fn text_body_xml_seeded(x: &Prefixes, body: Option<&TextBody>, seed: u64) -> String {
    text_body_xml_fields(x, body, seed, &mut 0)
}

fn text_body_xml_fields(x: &Prefixes, body: Option<&TextBody>, seed: u64, field: &mut u32) -> String {
    let (a, p) = (&x.a, &x.p);
    let Some(body) = body else {
        return format!(
            "<{p}txBody><{a}bodyPr rtlCol=\"0\" anchor=\"ctr\"/><{a}lstStyle/><{a}p><{a}pPr algn=\"ctr\"/><{a}endParaRPr lang=\"ja-JP\" altLang=\"en-US\" dirty=\"0\"/></{a}p></{p}txBody>"
        );
    };
    let mut attrs = String::new();
    write!(attrs, " wrap=\"{}\"", if body.wrap { "square" } else { "none" }).unwrap();
    let d = ibis_doc_model::Insets::default();
    let i = body.insets;
    for (name, v, dv) in [("lIns", i.left, d.left), ("tIns", i.top, d.top), ("rIns", i.right, d.right), ("bIns", i.bottom, d.bottom)] {
        if v != dv {
            write!(attrs, " {name}=\"{v}\"").unwrap();
        }
    }
    attrs.push_str(" rtlCol=\"0\"");
    match body.anchor {
        VerticalAnchor::Top => attrs.push_str(" anchor=\"t\""),
        VerticalAnchor::Middle => attrs.push_str(" anchor=\"ctr\""),
        VerticalAnchor::Bottom => attrs.push_str(" anchor=\"b\""),
    }
    if body.vertical {
        write!(attrs, " vert=\"{}\"", body.vertical_kind.ooxml()).unwrap();
    } else if body.sideways == 5_400_000 {
        attrs.push_str(" vert=\"vert\"");
    } else if body.sideways == 16_200_000 {
        attrs.push_str(" vert=\"vert270\"");
    }
    if body.rotation != 0 {
        attrs.push_str(&format!(" rot=\"{}\"", body.rotation));
    }
    if body.upright {
        attrs.push_str(" upright=\"1\"");
    }
    if body.columns > 1 {
        write!(attrs, " numCol=\"{}\"", body.columns).unwrap();
    }
    if body.column_spacing > 0 {
        write!(attrs, " spcCol=\"{}\"", body.column_spacing).unwrap();
    }
    let fit = match body.autofit {
        ibis_doc_model::TextAutofit::ResizeShape => format!("<{a}spAutoFit/>"),
        ibis_doc_model::TextAutofit::Shrink => {
            if body.font_scale != 100_000 {
                format!("<{a}normAutofit fontScale=\"{}\"/>", body.font_scale)
            } else {
                format!("<{a}normAutofit/>")
            }
        }
        ibis_doc_model::TextAutofit::None => {
            if body.font_scale != 100_000 {
                format!("<{a}normAutofit fontScale=\"{}\"/>", body.font_scale)
            } else {
                format!("<{a}noAutofit/>")
            }
        }
    };
    let warp_xml = if let Some(warp) = &body.warp {
        format!("<{a}prstTxWarp prst=\"{warp}\"><{a}avLst/></{a}prstTxWarp>")
    } else {
        String::new()
    };
    let mut s = format!("<{p}txBody><{a}bodyPr{attrs}>{warp_xml}{fit}</{a}bodyPr><{a}lstStyle/>");
    for paragraph in &body.paragraphs {
        s.push_str(&paragraph_xml(x, &body.style, paragraph, seed, field));
    }
    if body.paragraphs.is_empty() {
        s.push_str(&paragraph_xml(x, &body.style, &Paragraph::default(), seed, field));
    }
    write!(s, "</{p}txBody>").unwrap();
    s
}

pub(crate) fn auto_effects_xml(x: &Prefixes, shape: &ibis_doc_model::AutoShape) -> Result<String, Error> {
    let a = &x.a;
    let mut effects = String::new();
    if let Some((color, radius)) = shape.glow {
        if !(0..=i32::MAX as i64).contains(&radius) { return Err(unsupported("この光彩半径の範囲")); }
        write!(effects,"<{a}glow rad=\"{radius}\">{}</{a}glow>",color_xml(x,color)).unwrap();
    }
    if let Some(shadow) = &shape.shadow {
        if !(0..=i32::MAX as i64).contains(&shadow.blur) || !(0..=i32::MAX as i64).contains(&shadow.distance) || !(0..21_600_000).contains(&shadow.direction) { return Err(unsupported("この影の値")); }
        let mut attrs = format!(" blurRad=\"{}\" dist=\"{}\" dir=\"{}\"", shadow.blur, shadow.distance, shadow.direction);
        if shadow.sx != 100_000 {
            write!(attrs, " sx=\"{}\"", shadow.sx).unwrap();
        }
        if shadow.sy != 100_000 {
            write!(attrs, " sy=\"{}\"", shadow.sy).unwrap();
        }
        if shadow.kx != 0 {
            write!(attrs, " kx=\"{}\"", shadow.kx).unwrap();
        }
        if shadow.ky != 0 {
            write!(attrs, " ky=\"{}\"", shadow.ky).unwrap();
        }
        if shadow.align != ibis_doc_model::TileAlign::Bottom {
            let algn = match shadow.align {
                ibis_doc_model::TileAlign::TopLeft => "tl",
                ibis_doc_model::TileAlign::Top => "t",
                ibis_doc_model::TileAlign::TopRight => "tr",
                ibis_doc_model::TileAlign::Left => "l",
                ibis_doc_model::TileAlign::Center => "ctr",
                ibis_doc_model::TileAlign::Right => "r",
                ibis_doc_model::TileAlign::BottomLeft => "bl",
                ibis_doc_model::TileAlign::Bottom => "b",
                ibis_doc_model::TileAlign::BottomRight => "br",
            };
            write!(attrs, " algn=\"{algn}\"").unwrap();
        }
        if !shadow.rot_with_shape {
            write!(attrs, " rotWithShape=\"0\"").unwrap();
        }
        write!(effects,"<{a}outerShdw{attrs}>{}</{a}outerShdw>",color_xml(x,shadow.color)).unwrap();
    }
    if let Some(r) = &shape.reflection {
        let algn = match r.align {
            ibis_doc_model::TileAlign::TopLeft => "tl",
            ibis_doc_model::TileAlign::Top => "t",
            ibis_doc_model::TileAlign::TopRight => "tr",
            ibis_doc_model::TileAlign::Left => "l",
            ibis_doc_model::TileAlign::Center => "ctr",
            ibis_doc_model::TileAlign::Right => "r",
            ibis_doc_model::TileAlign::BottomLeft => "bl",
            ibis_doc_model::TileAlign::Bottom => "b",
            ibis_doc_model::TileAlign::BottomRight => "br",
        };
        write!(
            effects,
            "<{a}reflection blurRad=\"{}\" stA=\"{}\" stPos=\"{}\" endA=\"{}\" endPos=\"{}\" dist=\"{}\" dir=\"{}\" fadeDir=\"{}\" sx=\"{}\" sy=\"{}\" kx=\"{}\" ky=\"{}\" algn=\"{algn}\" rotWithShape=\"{}\"/>",
            r.blur, r.start_alpha, r.start_pos, r.end_alpha, r.end_pos, r.distance, r.direction, r.fade_direction, r.sx, r.sy, r.kx, r.ky, u8::from(r.rot_with_shape)
        )
        .unwrap();
    }
    if let Some(radius) = shape.soft_edge {
        if !(0..=i32::MAX as i64).contains(&radius) { return Err(unsupported("このぼかしの半径の範囲")); }
        write!(effects, "<{a}softEdge rad=\"{radius}\"/>").unwrap();
    }
    Ok(if effects.is_empty() { effects } else { format!("<{a}effectLst>{effects}</{a}effectLst>") })
}

/// Normalize an angle in 60,000ths of a degree to non-negative OOXML `ST_Angle` [0, 21_600_000).
/// PowerPoint crashes immediately when `<a:rot>` has negative angles (e.g. `lon="-2700000"`).
pub(crate) fn normalize_ooxml_angle(angle: i32) -> i32 {
    let m = 21_600_000;
    (angle % m + m) % m
}

pub(crate) fn scene3d_xml(x: &Prefixes, tilt: &ibis_doc_model::Tilt) -> String {
    let a = &x.a;
    let prst = tilt.camera_preset.map(|p| p.as_str()).unwrap_or("orthographicFront");
    format!(
        "<{a}scene3d><{a}camera prst=\"{prst}\"><{a}rot lat=\"{}\" lon=\"{}\" rev=\"{}\"/></{a}camera><{a}lightRig rig=\"threePt\" dir=\"t\"/></{a}scene3d>",
        normalize_ooxml_angle(tilt.lat),
        normalize_ooxml_angle(tilt.lon),
        normalize_ooxml_angle(tilt.rev)
    )
}

pub(crate) fn sp3d_xml(x: &Prefixes, tilt: Option<&ibis_doc_model::Tilt>, bevel: Option<&ibis_doc_model::Bevel>) -> String {
    let a = &x.a;
    let mut attrs = String::new();
    let mut children = String::new();
    if let Some(t) = tilt {
        if t.depth != 0 {
            use std::fmt::Write as _;
            write!(attrs, " z=\"{}\"", t.depth).ok();
        }
        if t.extrusion_h != 0 {
            use std::fmt::Write as _;
            write!(attrs, " extrusionH=\"{}\"", t.extrusion_h).ok();
        }
        if let Some(color) = t.extrusion_color {
            let hex = format!("{:06X}", color.rgb.0 & 0xffffff);
            children.push_str(&format!("<{a}extrusionClr><{a}srgbClr val=\"{hex}\"/></{a}extrusionClr>"));
        }
    }
    if let Some(b) = bevel {
        children.push_str(&format!("<{a}bevelT prst=\"{}\" w=\"{}\" h=\"{}\"/>", b.preset.as_str(), b.width, b.height));
    }
    if attrs.is_empty() && children.is_empty() {
        String::new()
    } else {
        format!("<{a}sp3d{attrs}>{children}</{a}sp3d>")
    }
}

/// The element for a new shape with drawing-object id `id`. A picture's
/// relationship id is `picture_rel`.
/// Check whether the current new-shape writer can serialize this shape.
/// This checks writer support, not fidelity to Office or opaque source data.
pub(super) fn check_shape_insertion(shape: &Shape) -> Result<(), Error> {
    let x = Prefixes { a: "a:".into(), p: "p:".into(), r: "r:".into(), missing: Vec::new(), p_ns: NS_P };
    shape_xml(&x, shape, 1, Some("rId1")).map(|_| ())
}

/// A new placeholder of a slide (日付・フッター・スライド番号 and the like):
/// `p:ph` as `placeholder` names it, its place and text formats left to
/// its layout (its place written only when it has its own), its text and
/// slide number fields without formats of their own.
pub(crate) fn placeholder_shape_xml(x: &Prefixes, shape: &Shape, placeholder: &ibis_doc_model::SlidePlaceholder, id: u32) -> Result<String, Error> {
    let (a, p) = (&x.a, &x.p);
    let ph = &placeholder.placeholder;
    let kind = ph.kind.as_deref();
    let name = match kind {
        Some("dt") => "日付プレースホルダー",
        Some("ftr") => "フッター プレースホルダー",
        Some("sldNum") => "スライド番号プレースホルダー",
        Some("title") | Some("ctrTitle") => "タイトル",
        _ => "テキスト プレースホルダー",
    };
    let mut attrs = String::new();
    if let Some(k) = kind {
        attrs.push_str(&format!(" type=\"{}\"", escape(k)));
    }
    if matches!(kind, Some("dt" | "ftr" | "sldNum")) {
        attrs.push_str(" sz=\"quarter\"");
    }
    if let Some(i) = &ph.index {
        attrs.push_str(&format!(" idx=\"{}\"", escape(i)));
    }
    let props = if placeholder.own_frame { format!("<{p}spPr>{}</{p}spPr>", xfrm_xml(x, &format!("{a}xfrm"), shape)) } else { format!("<{p}spPr/>") };
    let body = shape.text_body().ok_or_else(|| unsupported("文字の無いプレースホルダーの追加"))?;
    let rpr = format!("<{a}rPr lang=\"ja-JP\" altLang=\"en-US\"/>");
    let mut paragraphs = String::new();
    for (pi, para) in body.paragraphs.iter().enumerate() {
        let mut runs = String::new();
        for (ri, run) in para.runs.iter().enumerate() {
            let text = escape(&run.text);
            match run.style.field {
                Some(kind) => {
                    // The field's id, made from the shape and its place.
                    let seed = shape.id.0.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ ((pi as u64) << 20 | ri as u64);
                    let guid = format!("{{{:08X}-{:04X}-4{:03X}-8{:03X}-{:012X}}}", seed >> 32, (seed >> 16) & 0xffff, seed & 0xfff, (seed >> 4) & 0xfff, seed & 0xffff_ffff_ffff);
                    let rpr = match kind {
                        ibis_doc_model::TextField::DateTime { east_asian: false, .. } => format!("<{a}rPr lang=\"en-US\"/>"),
                        _ => rpr.clone(),
                    };
                    runs.push_str(&format!("<{a}fld id=\"{guid}\" type=\"{}\">{rpr}<{a}t>{text}</{a}t></{a}fld>", field_type(kind)));
                }
                None => runs.push_str(&format!("<{a}r>{rpr}<{a}t>{text}</{a}t></{a}r>")),
            }
        }
        paragraphs.push_str(&format!("<{a}p>{runs}<{a}endParaRPr lang=\"ja-JP\" altLang=\"en-US\"/></{a}p>"));
    }
    Ok(format!(
        "<{p}sp><{p}nvSpPr><{p}cNvPr id=\"{id}\" name=\"{name} {n}\"/><{p}cNvSpPr><{a}spLocks noGrp=\"1\"/></{p}cNvSpPr><{p}nvPr><{p}ph{attrs}/></{p}nvPr></{p}nvSpPr>{props}<{p}txBody><{a}bodyPr/><{a}lstStyle/>{paragraphs}</{p}txBody></{p}sp>",
        n = id - 1
    ))
}

pub(crate) fn shape_xml(x: &Prefixes, shape: &Shape, id: u32, picture_rel: Option<&str>) -> Result<String, Error> {
    shape_xml_with_glue(x, shape, id, picture_rel, None, None)
}

pub(crate) fn shape_xml_with_glue(
    x: &Prefixes,
    shape: &Shape,
    id: u32,
    picture_rel: Option<&str>,
    glue: Option<&ibis_doc_model::Glue>,
    id_map: Option<&std::collections::HashMap<u64, String>>,
) -> Result<String, Error> {
    let check_body = |body: &TextBody| -> Result<(), Error> {
        if !(1_000..=100_000).contains(&body.font_scale) {return Err(unsupported("文字縮小率の範囲"));}
        for paragraph in &body.paragraphs {
            if paragraph.bullet.as_ref().is_some_and(|bullet| bullet.picture.is_some()) {
                return Err(unsupported("画像箇条書きのある図形の追加"));
            }
        }
        Ok(())
    };
    if let Some(body) = shape.text_body() { check_body(body)?; }
    if let ShapeKind::Table(table) = &shape.kind {
        for row in &table.rows { for cell in &row.cells {
            check_body(&cell.text)?;
        } }
    }
    let (a, p) = (&x.a, &x.p);
    let xfrm = xfrm_xml(x, &format!("{a}xfrm"), shape);

    let cxn_glue_xml = match glue {
        Some(g) => {
            let mut s = String::new();
            if let Some((target_id, idx)) = g.start {
                let fallback = target_id.to_string();
                let native_id = id_map.and_then(|m| m.get(&target_id)).unwrap_or(&fallback);
                s.push_str(&format!("<{a}stCxn id=\"{native_id}\" idx=\"{idx}\"/>"));
            }
            if let Some((target_id, idx)) = g.end {
                let fallback = target_id.to_string();
                let native_id = id_map.and_then(|m| m.get(&target_id)).unwrap_or(&fallback);
                s.push_str(&format!("<{a}endCxn id=\"{native_id}\" idx=\"{idx}\"/>"));
            }
            if s.is_empty() {
                format!("<{p}cNvCxnSpPr/>")
            } else {
                format!("<{p}cNvCxnSpPr>{s}</{p}cNvCxnSpPr>")
            }
        }
        None => format!("<{p}cNvCxnSpPr/>"),
    };
    Ok(match &shape.kind {
        ShapeKind::TextBox(body) => format!(
            "<{p}sp><{p}nvSpPr><{p}cNvPr id=\"{id}\" name=\"TextBox {n}\"/>{}<{p}nvPr/></{p}nvSpPr><{p}spPr>{xfrm}<{a}prstGeom prst=\"rect\"><{a}avLst/></{a}prstGeom><{a}noFill/></{p}spPr>{}</{p}sp>",
            c_nv_sp_pr(x, true, shape.id),
            text_body_xml_seeded(x, Some(body), shape.id.0),
            n = id - 1,
        ),
        ShapeKind::Auto(auto) => {
            let is_connector = match &auto.geometry {
                Geometry::Formula(f) => matches!(
                    f.preset.as_deref(),
                    Some(
                        "bentConnector2"
                            | "bentConnector3"
                            | "bentConnector4"
                            | "bentConnector5"
                            | "curvedConnector2"
                            | "curvedConnector3"
                            | "curvedConnector4"
                            | "curvedConnector5"
                            | "straightConnector1"
                    )
                ),
                _ => false,
            };
            if is_connector && auto.text.is_none() {
                let name = match auto.geometry.preset_name() {
                    Some(p) if p.starts_with("bentConnector") => "Elbow Connector",
                    Some(p) if p.starts_with("curvedConnector") => "Curved Connector",
                    _ => "Straight Connector",
                };
                let line = line_xml(x, auto.stroke.as_ref());
                return Ok(format!(
                    "<{p}cxnSp><{p}nvCxnSpPr><{p}cNvPr id=\"{id}\" name=\"{name} {n}\"/>{cxn_glue_xml}<{p}nvPr/></{p}nvCxnSpPr><{p}spPr>{xfrm}{}{line}</{p}spPr></{p}cxnSp>",
                    geometry_xml(x, &auto.geometry)?,
                    n = id - 1,
                ));
            }
            let is_text_box = matches!(auto.geometry, Geometry::Rect) && auto.stroke.is_none() && auto.text.is_some();
            let (name, nv) = if is_text_box { ("TextBox", c_nv_sp_pr(x, true, shape.id)) } else { (geometry_name(&auto.geometry), c_nv_sp_pr(x, false, shape.id)) };
            let line = match &auto.stroke {
                Some(s) => line_xml(x, Some(s)),
                None if is_text_box => String::new(),
                None => line_xml(x, None),
            };
            let effects = auto_effects_xml(x, auto)?;
            let scene3d = auto.tilt.as_ref().map(|t| scene3d_xml(x, t)).unwrap_or_default();
            let sp3d = sp3d_xml(x, auto.tilt.as_ref(), auto.bevel.as_ref());
            format!(
                "<{p}sp><{p}nvSpPr><{p}cNvPr id=\"{id}\" name=\"{name} {n}\"/>{nv}<{p}nvPr/></{p}nvSpPr><{p}spPr>{xfrm}{}{}{line}{effects}{scene3d}{sp3d}</{p}spPr>{}</{p}sp>",
                geometry_xml(x, &auto.geometry)?,
                fill_xml(x, &auto.fill)?,
                text_body_xml_seeded(x, auto.text.as_ref(), shape.id.0),
                n = id - 1,
            )
        }
        ShapeKind::Line(stroke) => format!(
            "<{p}cxnSp><{p}nvCxnSpPr><{p}cNvPr id=\"{id}\" name=\"Straight Connector {n}\"/>{cxn_glue_xml}<{p}nvPr/></{p}nvCxnSpPr><{p}spPr>{xfrm}<{a}prstGeom prst=\"line\"><{a}avLst/></{a}prstGeom>{}</{p}spPr></{p}cxnSp>",
            line_xml(x, Some(stroke)),
            n = id - 1,
        ),
        ShapeKind::Picture(picture) => picture_xml(x, picture, &xfrm, id, shape.id, picture_rel.ok_or_else(|| unsupported("画像の追加"))?)?,
        ShapeKind::Table(table) => table_xml(x, shape, table, id)?,
        _ => return Err(unsupported("この種類の図形（グラフなど）の追加")),
    })
}

/// A new table (`p:graphicFrame` holding an `a:tbl`), each cell with its own
/// fill, lines and margins.
fn table_xml(x: &Prefixes, shape: &Shape, table: &ibis_doc_model::Table, id: u32) -> Result<String, Error> {
    let (a, p) = (&x.a, &x.p);
    // Which cells are covered, and from which side (hMerge: by a cell to
    // the left, vMerge: by one above).
    let mut covered: std::collections::HashMap<(usize, usize), (bool, bool)> = Default::default();
    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate().filter(|(_, x)| !x.merged) {
            for rr in r..r + cell.row_span.max(1) as usize {
                for cc in c..c + cell.column_span.max(1) as usize {
                    if (rr, cc) != (r, c) {
                        covered.insert((rr, cc), (cc > c, rr > r));
                    }
                }
            }
        }
    }
    let xfrm = xfrm_xml(x, &format!("{p}xfrm"), shape);
    let f_row = if table.style_options.first_row { "1" } else { "0" };
    let l_row = if table.style_options.last_row { "1" } else { "0" };
    let f_col = if table.style_options.first_col { "1" } else { "0" };
    let l_col = if table.style_options.last_col { "1" } else { "0" };
    let b_row = if table.style_options.band_row { "1" } else { "0" };
    let b_col = if table.style_options.band_col { "1" } else { "0" };
    let style_elem = match &table.style_id {
        Some(id) => format!("<{a}tableStyleId>{id}</{a}tableStyleId>"),
        None => String::new(),
    };
    let tbl_pr = format!("<{a}tblPr firstRow=\"{f_row}\" lastRow=\"{l_row}\" firstCol=\"{f_col}\" lastCol=\"{l_col}\" bandRow=\"{b_row}\" bandCol=\"{b_col}\">{style_elem}</{a}tblPr>");
    let mut s = format!(
        "<{p}graphicFrame><{p}nvGraphicFramePr><{p}cNvPr id=\"{id}\" name=\"表 {n}\"/><{p}cNvGraphicFramePr><{a}graphicFrameLocks noGrp=\"1\"/></{p}cNvGraphicFramePr><{p}nvPr/></{p}nvGraphicFramePr>\
         {xfrm}\
         <{a}graphic><{a}graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><{a}tbl>{tbl_pr}<{a}tblGrid>",
        n = id - 1,
    );
    for w in &table.columns {
        write!(s, "<{a}gridCol w=\"{w}\"/>").unwrap();
    }
    write!(s, "</{a}tblGrid>").unwrap();
    let mut field = 0;
    for (r, row) in table.rows.iter().enumerate() {
        write!(s, "<{a}tr h=\"{}\">", row.height).unwrap();
        for (c, cell) in row.cells.iter().enumerate() {
            // The cell's text, as an `a:txBody`.
            let body = text_body_xml_fields(x, Some(&cell.text), shape.id.0, &mut field);
            let body = body.replacen(&format!("<{p}txBody>"), &format!("<{a}txBody>"), 1).replace(&format!("</{p}txBody>"), &format!("</{a}txBody>"));
            let i = cell.text.insets;
            let anchor = match cell.text.anchor {
                ibis_doc_model::VerticalAnchor::Top => "t",
                ibis_doc_model::VerticalAnchor::Middle => "ctr",
                ibis_doc_model::VerticalAnchor::Bottom => "b",
            };
            let mut spans = String::new();
            if cell.column_span > 1 {
                write!(spans, " gridSpan=\"{}\"", cell.column_span).unwrap();
            }
            if cell.row_span > 1 {
                write!(spans, " rowSpan=\"{}\"", cell.row_span).unwrap();
            }
            if cell.merged {
                let (h, v) = covered.get(&(r, c)).copied().unwrap_or((true, false));
                if h {
                    spans.push_str(" hMerge=\"1\"");
                }
                if v {
                    spans.push_str(" vMerge=\"1\"");
                }
            }
            let vert = if cell.text.vertical {
                format!(" vert=\"{}\"", cell.text.vertical_kind.ooxml())
            } else {
                match cell.text.sideways {
                    5_400_000 => " vert=\"vert\"".to_string(),
                    16_200_000 => " vert=\"vert270\"".to_string(),
                    _ => String::new(),
                }
            };
            write!(s, "<{a}tc{spans}>{body}<{a}tcPr marL=\"{}\" marR=\"{}\" marT=\"{}\" marB=\"{}\" anchor=\"{anchor}\"{vert}>", i.left, i.right, i.top, i.bottom).unwrap();
            let b = &cell.borders;
            for (name, line) in [("lnL", &b.left), ("lnR", &b.right), ("lnT", &b.top), ("lnB", &b.bottom), ("lnTlToBr", &b.tl_to_br), ("lnBlToTr", &b.bl_to_tr)] {
                let line = line_xml(x, line.as_ref()).replacen(&format!("<{a}ln"), &format!("<{a}{name}"), 1).replacen(&format!("</{a}ln>"), &format!("</{a}{name}>"), 1);
                s.push_str(&line);
            }
            write!(s, "{}</{a}tcPr></{a}tc>", fill_xml(x, &cell.fill)?).unwrap();
        }
        write!(s, "</{a}tr>").unwrap();
    }
    write!(s, "</{a}tbl></{a}graphicData></{a}graphic></{p}graphicFrame>").unwrap();
    Ok(s)
}

thread_local! {
    static ASPECT_LOCKED: std::cell::RefCell<std::collections::HashSet<ibis_doc_model::ShapeId>> = Default::default();
}

/// The shapes of the slide being written that keep their proportions.
pub(crate) fn set_aspect_locked(locked: std::collections::HashSet<ibis_doc_model::ShapeId>) {
    ASPECT_LOCKED.with(|c| *c.borrow_mut() = locked);
}

/// A new shape's `p:cNvSpPr` (a text box says so; its locks keep the proportions if it does).
fn c_nv_sp_pr(x: &Prefixes, text_box: bool, shape: ibis_doc_model::ShapeId) -> String {
    let (a, p) = (&x.a, &x.p);
    let tx = if text_box { " txBox=\"1\"" } else { "" };
    match aspect_attr(shape) {
        "" => format!("<{p}cNvSpPr{tx}/>"),
        lock => format!("<{p}cNvSpPr{tx}><{a}spLocks{lock}/></{p}cNvSpPr>"),
    }
}

/// `noChangeAspect` for a new shape's locks, if it keeps its proportions.
fn aspect_attr(shape: ibis_doc_model::ShapeId) -> &'static str {
    if ASPECT_LOCKED.with(|c| c.borrow().contains(&shape)) { " noChangeAspect=\"1\"" } else { "" }
}

thread_local! {
    static CLIP_RELS: std::cell::RefCell<std::collections::HashMap<ibis_doc_model::ShapeId, (String, String)>> = Default::default();
    static MEDIA_RELS: std::cell::RefCell<std::collections::HashMap<ibis_doc_model::MediaId, String>> = Default::default();
    static SVG_MEDIA: std::cell::RefCell<std::collections::HashSet<ibis_doc_model::MediaId>> = Default::default();
}

/// The relationships of the new clips (video, audio) of the slide being
/// written, by shape: the file as a video or audio, and as 2010 media.
pub(crate) fn set_clip_rels(rels: std::collections::HashMap<ibis_doc_model::ShapeId, (String, String)>) {
    CLIP_RELS.with(|c| *c.borrow_mut() = rels);
}

/// The relationship IDs of image media for the slide being written.
pub(crate) fn set_media_rels(rels: std::collections::HashMap<ibis_doc_model::MediaId, String>) {
    MEDIA_RELS.with(|m| *m.borrow_mut() = rels);
}

/// The SVG image media IDs for the slide being written.
pub(crate) fn set_svg_media(svg: std::collections::HashSet<ibis_doc_model::MediaId>) {
    SVG_MEDIA.with(|s| *s.borrow_mut() = svg);
}

pub(crate) fn is_svg_media(media: ibis_doc_model::MediaId) -> bool {
    SVG_MEDIA.with(|s| s.borrow().contains(&media))
}

fn picture_xml(x: &Prefixes, picture: &Picture, xfrm: &str, id: u32, shape: ibis_doc_model::ShapeId, rel: &str) -> Result<String, Error> {
    let (a, p, r) = (&x.a, &x.p, &x.r);
    let mut effects = String::new();
    if let Some((dark, light)) = picture.duotone {
        write!(effects, "<{a}duotone>{}{}</{a}duotone>", color_xml(x, Color::opaque(dark)), color_xml(x, Color::opaque(light))).unwrap();
    }
    if let Some(color) = picture.clear {
        write!(effects, "<{a}clrChange><{a}clrFrom>{}</{a}clrFrom><{a}clrTo>{}</{a}clrTo></{a}clrChange>", color_xml(x, Color::opaque(color)), color_xml(x, Color { rgb: color, alpha: 0 })).unwrap();
    }
    if let Some((bright, contrast)) = picture.luminance {
        let mut attrs = String::new();
        if bright != 0 {
            write!(attrs, r#" bright="{bright}""#).unwrap();
        }
        if contrast != 0 {
            write!(attrs, r#" contrast="{contrast}""#).unwrap();
        }
        if !attrs.is_empty() {
            write!(effects, "<{a}lum{attrs}/>").unwrap();
        }
    }
    let is_svg = is_svg_media(picture.media);
    let mut exts = Vec::new();
    if is_svg {
        exts.push(format!(
            r#"<{a}ext uri="{{96DAC542-7B81-4868-9F9E-1EA1F31DC1F3}}"><asvg:svgBlip xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" {r}embed="{rel}"/></{a}ext>"#
        ));
    }
    if let Some(ref effect) = picture.artistic_effect {
        let eff_xml = artistic_effect_xml(effect);
        exts.push(format!(
            r#"<{a}ext uri="{{BEBA8EAE-BF5A-486C-A8C5-ECC9F3942E4B}}"><a14:imgProps xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main"><a14:imgLayer><a14:imgEffect>{eff_xml}</a14:imgEffect></a14:imgLayer></a14:imgProps></{a}ext>"#
        ));
    }
    let blip_ext = if exts.is_empty() {
        String::new()
    } else {
        format!("<{a}extLst>{}</{a}extLst>", exts.concat())
    };
    let c = picture.crop;
    let crop = if c == ibis_doc_model::Crop::default() {
        String::new()
    } else {
        let mut s = format!("<{a}srcRect");
        for (name, v) in [("l", c.left), ("t", c.top), ("r", c.right), ("b", c.bottom)] {
            if v != 0 {
                write!(s, " {name}=\"{v}\"").unwrap();
            }
        }
        s.push_str("/>");
        s
    };
    let geometry = if picture.corner > 0 {
        format!("<{a}prstGeom prst=\"roundRect\"><{a}avLst><{a}gd name=\"adj\" fmla=\"val {}\"/></{a}avLst></{a}prstGeom>", picture.corner)
    } else {
        format!("<{a}prstGeom prst=\"rect\"><{a}avLst/></{a}prstGeom>")
    };
    let line = picture.stroke.as_ref().map(|s| line_xml(x, Some(s))).unwrap_or_default();
    let sp_effects = picture_effects_xml(x, picture)?;
    // A video or audio clip: `a:videoFile`/`a:audioFile` and `p14:media`,
    // clicked to play (`ppaction://media`), as PowerPoint writes them.
    let clip = picture.clip.clone().and_then(|clip| CLIP_RELS.with(|c| c.borrow().get(&shape).cloned()).map(|rels| (clip, rels)));
    let (name, nv_pr, hlink) = match clip {
        Some((clip, (link, embed))) => {
            let (tag, name) = if clip.audio { ("audioFile", "Audio") } else { ("videoFile", "Movie") };
            let mut children = String::new();
            if clip.options.trim_start_ms > 0 || clip.options.trim_end_ms > 0 {
                let st_attr = if clip.options.trim_start_ms > 0 { format!(" st=\"{}\"", clip.options.trim_start_ms) } else { String::new() };
                let end_attr = if clip.options.trim_end_ms > 0 { format!(" end=\"{}\"", clip.options.trim_end_ms) } else { String::new() };
                children.push_str(&format!("<p14:trim{st_attr}{end_attr}/>"));
            }
            if clip.options.fade_in_ms > 0 || clip.options.fade_out_ms > 0 {
                let in_attr = if clip.options.fade_in_ms > 0 { format!(" in=\"{}\"", clip.options.fade_in_ms) } else { String::new() };
                let out_attr = if clip.options.fade_out_ms > 0 { format!(" out=\"{}\"", clip.options.fade_out_ms) } else { String::new() };
                children.push_str(&format!("<p14:fade{in_attr}{out_attr}/>"));
            }
            if !clip.options.bookmarks.is_empty() {
                children.push_str("<p14:bmkLst>");
                for bmk in &clip.options.bookmarks {
                    children.push_str(&format!("<p14:bmk name=\"{}\" time=\"{}\"/>", escape(&bmk.name), bmk.time_ms));
                }
                children.push_str("</p14:bmkLst>");
            }
            if !clip.options.captions.is_empty() {
                children.push_str("<p14:captionTracks>");
                for track in &clip.options.captions {
                    children.push_str(&format!(
                        "<p14:captionTrack label=\"{}\" lang=\"{}\" vtt=\"{}\"/>",
                        escape_attr(&track.label),
                        escape_attr(&track.lang),
                        escape_attr(&track.vtt)
                    ));
                }
                children.push_str("</p14:captionTracks>");
            }
            let media_tag = if children.is_empty() {
                format!("<p14:media xmlns:p14=\"http://schemas.microsoft.com/office/powerpoint/2010/main\" {r}embed=\"{embed}\"/>")
            } else {
                format!("<p14:media xmlns:p14=\"http://schemas.microsoft.com/office/powerpoint/2010/main\" {r}embed=\"{embed}\">{children}</p14:media>")
            };
            // A linked file (no embedded copy): the file only.
            let nv_pr = if embed.is_empty() {
                format!("<{p}nvPr><{a}{tag} {r}link=\"{link}\"/></{p}nvPr>")
            } else {
                format!("<{p}nvPr><{a}{tag} {r}link=\"{link}\"/><{p}extLst><{p}ext uri=\"{{DAA4B4D4-6D71-4841-9C94-3DE7FCFB9230}}\">{media_tag}</{p}ext></{p}extLst></{p}nvPr>")
            };
            (name, nv_pr, format!("<{a}hlinkClick {r}id=\"\" action=\"ppaction://media\"/>"))
        }
        None => ("Picture", format!("<{p}nvPr/>"), String::new()),
    };
    let lock = aspect_attr(shape);
    let shape_name = if !picture.name.is_empty() {
        picture.name.clone()
    } else {
        format!("{name} {n}", n = id - 1)
    };
    Ok(format!(
        "<{p}pic><{p}nvPicPr><{p}cNvPr id=\"{id}\" name=\"{shape_name}\">{hlink}</{p}cNvPr><{p}cNvPicPr><{a}picLocks{lock}/></{p}cNvPicPr>{nv_pr}</{p}nvPicPr><{p}blipFill><{a}blip {r}embed=\"{rel}\">{effects}{blip_ext}</{a}blip>{crop}<{a}stretch><{a}fillRect/></{a}stretch></{p}blipFill><{p}spPr>{xfrm}{geometry}{line}{sp_effects}</{p}spPr></{p}pic>",
    ))
}

fn picture_effects_xml(x: &Prefixes, picture: &Picture) -> Result<String, Error> {
    let a = &x.a;
    let mut effects = String::new();
    if let Some(shadow) = &picture.shadow {
        if !(0..=i32::MAX as i64).contains(&shadow.blur) || !(0..=i32::MAX as i64).contains(&shadow.distance) || !(0..21_600_000).contains(&shadow.direction) { return Err(unsupported("この影の値")); }
        let mut attrs = format!(" blurRad=\"{}\" dist=\"{}\" dir=\"{}\"", shadow.blur, shadow.distance, shadow.direction);
        if shadow.sx != 100_000 {
            write!(attrs, " sx=\"{}\"", shadow.sx).unwrap();
        }
        if shadow.sy != 100_000 {
            write!(attrs, " sy=\"{}\"", shadow.sy).unwrap();
        }
        if shadow.kx != 0 {
            write!(attrs, " kx=\"{}\"", shadow.kx).unwrap();
        }
        if shadow.ky != 0 {
            write!(attrs, " ky=\"{}\"", shadow.ky).unwrap();
        }
        if shadow.align != ibis_doc_model::TileAlign::Bottom {
            let algn = match shadow.align {
                ibis_doc_model::TileAlign::TopLeft => "tl",
                ibis_doc_model::TileAlign::Top => "t",
                ibis_doc_model::TileAlign::TopRight => "tr",
                ibis_doc_model::TileAlign::Left => "l",
                ibis_doc_model::TileAlign::Center => "ctr",
                ibis_doc_model::TileAlign::Right => "r",
                ibis_doc_model::TileAlign::BottomLeft => "bl",
                ibis_doc_model::TileAlign::Bottom => "b",
                ibis_doc_model::TileAlign::BottomRight => "br",
            };
            write!(attrs, " algn=\"{algn}\"").unwrap();
        }
        if !shadow.rot_with_shape {
            write!(attrs, " rotWithShape=\"0\"").unwrap();
        }
        write!(effects,"<{a}outerShdw{attrs}>{}</{a}outerShdw>",color_xml(x,shadow.color)).unwrap();
    }
    if let Some(r) = &picture.reflection {
        let algn = match r.align {
            ibis_doc_model::TileAlign::TopLeft => "tl",
            ibis_doc_model::TileAlign::Top => "t",
            ibis_doc_model::TileAlign::TopRight => "tr",
            ibis_doc_model::TileAlign::Left => "l",
            ibis_doc_model::TileAlign::Center => "ctr",
            ibis_doc_model::TileAlign::Right => "r",
            ibis_doc_model::TileAlign::BottomLeft => "bl",
            ibis_doc_model::TileAlign::Bottom => "b",
            ibis_doc_model::TileAlign::BottomRight => "br",
        };
        write!(
            effects,
            "<{a}reflection blurRad=\"{}\" stA=\"{}\" stPos=\"{}\" endA=\"{}\" endPos=\"{}\" dist=\"{}\" dir=\"{}\" fadeDir=\"{}\" sx=\"{}\" sy=\"{}\" kx=\"{}\" ky=\"{}\" algn=\"{algn}\" rotWithShape=\"{}\"/>",
            r.blur, r.start_alpha, r.start_pos, r.end_alpha, r.end_pos, r.distance, r.direction, r.fade_direction, r.sx, r.sy, r.kx, r.ky, u8::from(r.rot_with_shape)
        )
        .unwrap();
    }
    if let Some(radius) = picture.soft_edge {
        if !(0..=i32::MAX as i64).contains(&radius) { return Err(unsupported("このぼかしの半径の範囲")); }
        write!(effects, "<{a}softEdge rad=\"{radius}\"/>").unwrap();
    }
    Ok(if effects.is_empty() { effects } else { format!("<{a}effectLst>{effects}</{a}effectLst>") })
}

fn geometry_name(geometry: &Geometry) -> &'static str {
    match geometry {
        Geometry::Rect => "Rectangle",
        Geometry::RoundRect { .. } => "Rounded Rectangle",
        Geometry::Ellipse => "Oval",
        Geometry::Formula(_) => "Shape",
    }
}

/// A new slide part: an empty shape tree with the given placeholders
/// (already written), following its layout's colour mapping.
pub(crate) fn slide_part_xml(placeholders: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<p:sld xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p=\"{NS_P}\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{placeholders}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"
    )
}

/// An empty placeholder of a new slide, for a layout placeholder whose
/// `p:ph` has the attributes `ph_attrs` (already escaped, with a leading
/// space) and whose name is `name`.
pub(crate) fn placeholder_xml(id: u32, name: &str, ph_attrs: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph{ph_attrs}/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"ja-JP\" altLang=\"en-US\"/></a:p></p:txBody></p:sp>",
        escape(name)
    )
}

pub(crate) fn artistic_effect_xml(effect: &ibis_doc_model::ArtisticEffect) -> String {
    use ibis_doc_model::ArtisticEffect;
    match *effect {
        ArtisticEffect::Blur { radius } => format!(r#"<a14:artisticBlur radius="{radius}"/>"#),
        ArtisticEffect::Cement { crack_spacing, trans } => format!(r#"<a14:artisticCement crackSpacing="{crack_spacing}" trans="{trans}"/>"#),
        ArtisticEffect::ChalkSketch { pressure, trans } => format!(r#"<a14:artisticChalkSketch pressure="{pressure}" trans="{trans}"/>"#),
        ArtisticEffect::CrisscrossEtching { pressure, trans } => format!(r#"<a14:artisticCrisscrossEtching pressure="{pressure}" trans="{trans}"/>"#),
        ArtisticEffect::Cutout { number_of_shades, trans } => format!(r#"<a14:artisticCutout numberOfShades="{number_of_shades}" trans="{trans}"/>"#),
        ArtisticEffect::FilmGrain { grain_size, trans } => format!(r#"<a14:artisticFilmGrain grainSize="{grain_size}" trans="{trans}"/>"#),
        ArtisticEffect::Glass { scaling, trans } => format!(r#"<a14:artisticGlass scaling="{scaling}" trans="{trans}"/>"#),
        ArtisticEffect::GlowDiffused { intensity, trans } => format!(r#"<a14:artisticGlowDiffused intensity="{intensity}" trans="{trans}"/>"#),
        ArtisticEffect::GlowEdges { smoothness, trans } => format!(r#"<a14:artisticGlowEdges smoothness="{smoothness}" trans="{trans}"/>"#),
        ArtisticEffect::LightScreen { grid_size, trans } => format!(r#"<a14:artisticLightScreen gridSize="{grid_size}" trans="{trans}"/>"#),
        ArtisticEffect::LineDrawing { pencil_size, trans } => format!(r#"<a14:artisticLineDrawing pencilSize="{pencil_size}" trans="{trans}"/>"#),
        ArtisticEffect::Marker { size, trans } => format!(r#"<a14:artisticMarker size="{size}" trans="{trans}"/>"#),
        ArtisticEffect::MosaicBubbles { pressure, trans } => format!(r#"<a14:artisticMosiaicBubbles pressure="{pressure}" trans="{trans}"/>"#),
        ArtisticEffect::PaintBrush { brush_size, trans } => format!(r#"<a14:artisticPaintBrush brushSize="{brush_size}" trans="{trans}"/>"#),
        ArtisticEffect::PaintStrokes { intensity, trans } => format!(r#"<a14:artisticPaintStrokes intensity="{intensity}" trans="{trans}"/>"#),
        ArtisticEffect::PastelsSmooth { scaling, trans } => format!(r#"<a14:artisticPastelsSmooth scaling="{scaling}" trans="{trans}"/>"#),
        ArtisticEffect::PencilGrayscale { pencil_size, trans } => format!(r#"<a14:artisticPencilGrayscale pencilSize="{pencil_size}" trans="{trans}"/>"#),
        ArtisticEffect::PencilSketch { pressure, trans } => format!(r#"<a14:artisticPencilSketch pressure="{pressure}" trans="{trans}"/>"#),
        ArtisticEffect::Photocopy { detail, trans } => format!(r#"<a14:artisticPhotocopy detail="{detail}" trans="{trans}"/>"#),
        ArtisticEffect::PlasticWrap { smoothness, trans } => format!(r#"<a14:artisticPlasticWrap smoothness="{smoothness}" trans="{trans}"/>"#),
        ArtisticEffect::Texturizer { scaling, trans } => format!(r#"<a14:artisticTexturizer scaling="{scaling}" trans="{trans}"/>"#),
        ArtisticEffect::WatercolorSponge { brush_size, trans } => format!(r#"<a14:artisticWatercolorSponge brushSize="{brush_size}" trans="{trans}"/>"#),
    }
}

