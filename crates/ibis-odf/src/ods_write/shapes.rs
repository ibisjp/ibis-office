// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing shapes of a sheet as ODF shapes in the cell they start in (ODF 1.3 chapter 10): rectangles, ellipses,
//! rounded rectangles and the other presets (`draw:custom-shape`), text boxes (a frame with `draw:text-box`) and
//! straight lines (`draw:line`), with their fill, outline and text.

use ibis_doc_model::{AutoShape, Fill, Geometry, Shape, ShapeKind, Stroke, TextAlign, TextBody, VerticalAnchor};

use super::{cm, esc};

fn hex(rgb: ibis_doc_model::Rgb) -> String {
    format!("#{:06x}", rgb.0)
}

/// The style of a shape: its fill, outline and the text's look.
fn style_xml(name: &str, fill: &Fill, stroke: Option<&Stroke>, text: Option<&TextBody>) -> String {
    let mut g = String::new();
    match fill {
        Fill::Solid(c) => g.push_str(&format!(" draw:fill=\"solid\" draw:fill-color=\"{}\"", hex(c.rgb))),
        _ => g.push_str(" draw:fill=\"none\""),
    }
    match stroke {
        Some(s) => g.push_str(&format!(" draw:stroke=\"{}\" svg:stroke-width=\"{}\" svg:stroke-color=\"{}\"", if s.dashed { "dash" } else { "solid" }, cm(s.width.max(1)), hex(s.color.rgb))),
        None => g.push_str(" draw:stroke=\"none\""),
    }
    let (mut para, mut t) = (String::new(), String::new());
    if let Some(body) = text {
        g.push_str(match body.anchor {
            VerticalAnchor::Top => " draw:textarea-vertical-align=\"top\"",
            VerticalAnchor::Middle => " draw:textarea-vertical-align=\"middle\"",
            VerticalAnchor::Bottom => " draw:textarea-vertical-align=\"bottom\"",
        });
        let run = body.paragraphs.iter().flat_map(|p| p.runs.iter()).next().map(|r| &r.style);
        let size = run.and_then(|r| r.font_size).unwrap_or(body.style.font_size);
        let bold = run.and_then(|r| r.bold).unwrap_or(body.style.bold);
        let color = run.and_then(|r| r.color).unwrap_or(body.style.color);
        t.push_str(&format!(" fo:font-size=\"{}pt\" fo:color=\"{}\"", size as f64 / 100.0, hex(color)));
        if bold {
            t.push_str(" fo:font-weight=\"bold\"");
        }
        let align = body.paragraphs.first().and_then(|p| p.align).unwrap_or(body.style.align);
        para.push_str(match align {
            TextAlign::Center => " fo:text-align=\"center\"",
            TextAlign::Right => " fo:text-align=\"end\"",
            TextAlign::Justify | TextAlign::Distributed => " fo:text-align=\"justify\"",
            TextAlign::Left => " fo:text-align=\"start\"",
        });
    }
    format!("<style:style style:name=\"{name}\" style:family=\"graphic\" style:parent-style-name=\"standard\"><style:graphic-properties{g}/><style:paragraph-properties{para}/><style:text-properties{t}/></style:style>")
}

fn paragraphs(text: Option<&TextBody>) -> String {
    match text {
        Some(body) => body.paragraphs.iter().map(|p| format!("<text:p>{}</text:p>", esc(&p.runs.iter().map(|r| r.text.as_str()).collect::<String>()))).collect(),
        None => "<text:p/>".to_string(),
    }
}

fn custom_shape(a: &AutoShape) -> Option<String> {
    let ty = match &a.geometry {
        Geometry::Rect => "rectangle".to_string(),
        Geometry::RoundRect { .. } => "round-rectangle".to_string(),
        Geometry::Ellipse => "ellipse".to_string(),
        Geometry::Formula(f) => format!("ooxml-{}", f.preset.as_deref()?),
    };
    Some(ty)
}

/// The XML of `shape` placed with `(x, y)` its offset in its cell, and the name of the style it needs (appended to
/// `styles`); `None` for a shape that is not a drawing shape (or not written).
pub(super) fn shape_xml(shape: &Shape, index: usize, x: i64, y: i64, styles: &mut String) -> Option<String> {
    let f = shape.frame;
    let style = format!("gs{index}");
    let z = format!("draw:z-index=\"{index}\" draw:name=\"Shape {index}\" draw:style-name=\"{style}\"");
    match &shape.kind {
        ShapeKind::Auto(a) => {
            let ty = custom_shape(a)?;
            styles.push_str(&style_xml(&style, &a.fill, a.stroke.as_ref(), a.text.as_ref()));
            Some(format!(
                "<draw:custom-shape {z} svg:width=\"{}\" svg:height=\"{}\" svg:x=\"{}\" svg:y=\"{}\">{}<draw:enhanced-geometry draw:type=\"{ty}\"/></draw:custom-shape>",
                cm(f.width),
                cm(f.height),
                cm(x),
                cm(y),
                paragraphs(a.text.as_ref())
            ))
        }
        ShapeKind::TextBox(body) => {
            styles.push_str(&style_xml(&style, &Fill::None, None, Some(body)));
            Some(format!("<draw:frame {z} svg:width=\"{}\" svg:height=\"{}\" svg:x=\"{}\" svg:y=\"{}\"><draw:text-box>{}</draw:text-box></draw:frame>", cm(f.width), cm(f.height), cm(x), cm(y), paragraphs(Some(body))))
        }
        ShapeKind::Line(stroke) => {
            styles.push_str(&style_xml(&style, &Fill::None, Some(stroke), None));
            let (y1, y2) = if shape.flip_v { (y + f.height, y) } else { (y, y + f.height) };
            let (x1, x2) = if shape.flip_h { (x + f.width, x) } else { (x, x + f.width) };
            Some(format!("<draw:line {z} svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\"><text:p/></draw:line>", cm(x1), cm(y1), cm(x2), cm(y2)))
        }
        _ => None,
    }
}
