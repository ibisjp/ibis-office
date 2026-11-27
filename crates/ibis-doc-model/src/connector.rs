// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コネクタ (DOCX-IMAGE-014): the connection sites of the shapes drawn on a page, and the connectors that follow the shapes
//! they are glued to (`a:stCxn`, `a:endCxn`).
//!
//! Only shapes placed relative to the page (no rotation) have sites that can be found here. The sites are numbered as Word
//! numbers them: a rectangle (and a rounded rectangle, a diamond, a text box) has four, counted from the top
//! anticlockwise (top, left, bottom, right); an ellipse has eight (top, top left, left, bottom left, bottom, bottom right,
//! right, top right).

use crate::flow::{Block, FlowDocument, FlowRunKind};
use crate::geometry::Emu;
use crate::model::{Shape, ShapeKind};

/// Whether `shape` is placed at a place on the page or slide that is known (not turned).
fn on_page(shape: &Shape) -> bool {
    shape.rotation == 0 && (shape.floating.is_none() || shape.floating.as_ref().is_some_and(|f| f.relative_h == "page" && f.relative_v == "page" && f.align_h.is_none() && f.align_v.is_none()))
}

/// The connection sites of `shape` as points of the page, numbered as Word does. Empty when the shape's place is not known.
pub fn sites(shape: &Shape) -> Vec<(Emu, Emu)> {
    if !on_page(shape) {
        return Vec::new();
    }
    let f = shape.frame;
    let (x, y, w, h) = (f.x, f.y, f.width, f.height);
    let four = vec![(x + w / 2, y), (x, y + h / 2), (x + w / 2, y + h), (x + w, y + h / 2)];
    match &shape.kind {
        ShapeKind::Auto(a) => match a.geometry.preset_name() {
            Some("ellipse") => {
                // The corners of the square the ellipse touches at 45 degrees: 1 - cos(45 degrees) = 0.2929 of the half.
                let (dx, dy) = ((w as f64 * 0.146_447) as Emu, (h as f64 * 0.146_447) as Emu);
                vec![(x + w / 2, y), (x + dx, y + dy), (x, y + h / 2), (x + dx, y + h - dy), (x + w / 2, y + h), (x + w - dx, y + h - dy), (x + w, y + h / 2), (x + w - dx, y + dy)]
            }
            Some("rect" | "roundRect" | "diamond" | "flowChartProcess" | "flowChartDecision" | "flowChartAlternateProcess") => four,
            _ => Vec::new(),
        },
        ShapeKind::TextBox(_) => four,
        _ => Vec::new(),
    }
}

/// Every shape in the body's paragraphs, with the paragraph's index and the shape's offset in it.
fn shapes(doc: &FlowDocument) -> Vec<(usize, usize, &Shape)> {
    let mut out = Vec::new();
    for (bi, block) in doc.body.iter().enumerate() {
        if let Block::Paragraph(p) = block {
            for (ri, run) in p.runs.iter().enumerate() {
                if let FlowRunKind::Shape(s) = &run.kind {
                    out.push((bi, ri, &**s));
                }
            }
        }
    }
    out
}

/// The connection site nearest to the point (`x`, `y`) of the page within `radius`: the shape's id, the site's number and
/// the point. Connectors themselves are not asked.
pub fn nearest_site(doc: &FlowDocument, x: Emu, y: Emu, radius: Emu) -> Option<(u64, u32, (Emu, Emu))> {
    let mut best: Option<(i128, (u64, u32, (Emu, Emu)))> = None;
    for (_, _, s) in shapes(doc) {
        if is_connector(s) {
            continue;
        }
        for (i, p) in sites(s).into_iter().enumerate() {
            let d = (p.0 as i128 - x as i128).pow(2) + (p.1 as i128 - y as i128).pow(2);
            if d <= (radius as i128).pow(2) && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, (s.id.0, i as u32, p)));
            }
        }
    }
    best.map(|b| b.1)
}

/// The connection site nearest to the point (`x`, `y`) of a slide within `radius`:
/// returns (shape's id as u64, site number, (site_x, site_y)). Connectors themselves are not asked.
pub fn nearest_site_on_slide<'a>(shapes: impl IntoIterator<Item = &'a Shape>, x: Emu, y: Emu, radius: Emu) -> Option<(u64, u32, (Emu, Emu))> {
    let mut best: Option<(i128, (u64, u32, (Emu, Emu)))> = None;
    for s in shapes {
        if is_connector(s) {
            continue;
        }
        for (i, p) in sites(s).into_iter().enumerate() {
            let d = (p.0 as i128 - x as i128).pow(2) + (p.1 as i128 - y as i128).pow(2);
            if d <= (radius as i128).pow(2) && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, (s.id.0, i as u32, p)));
            }
        }
    }
    best.map(|b| b.1)
}

/// Whether `shape` is a line or a shape of a connector's geometry.
pub fn is_connector(shape: &Shape) -> bool {
    match &shape.kind {
        ShapeKind::Line(_) => true,
        ShapeKind::Auto(a) => a.geometry.preset_name().is_some_and(|n| n.contains("Connector")),
        _ => false,
    }
}

/// The ends of a connector as points of the page: where it starts and where it ends.
pub fn ends(shape: &Shape) -> ((Emu, Emu), (Emu, Emu)) {
    let f = shape.frame;
    let start = (if shape.flip_h { f.x + f.width } else { f.x }, if shape.flip_v { f.y + f.height } else { f.y });
    let end = (if shape.flip_h { f.x } else { f.x + f.width }, if shape.flip_v { f.y } else { f.y + f.height });
    (start, end)
}

/// The frame and flips that run a connector from `start` to `end`.
pub fn frame_between(start: (Emu, Emu), end: (Emu, Emu)) -> (crate::geometry::EmuRect, bool, bool) {
    let (x, y) = (start.0.min(end.0), start.1.min(end.1));
    (crate::geometry::EmuRect::new(x, y, (start.0 - end.0).abs(), (start.1 - end.1).abs()), start.0 > end.0, start.1 > end.1)
}

/// The connectors that must change to follow the shapes they are glued to: the paragraph's index, the shape's offset in it,
/// and the connector as it should be.
pub fn rerouted(doc: &FlowDocument) -> Vec<(usize, usize, Shape)> {
    let all = shapes(doc);
    let mut out = Vec::new();
    for &(bi, ri, s) in &all {
        let Some(f) = s.floating.as_ref() else { continue };
        if !is_connector(s) || !on_page(s) || (f.glue.start.is_none() && f.glue.end.is_none()) {
            continue;
        }
        let site = |end: Option<(u64, u32)>| -> Option<(Emu, Emu)> {
            let (id, idx) = end?;
            all.iter().find(|(_, _, t)| t.id.0 == id).and_then(|(_, _, t)| sites(t).get(idx as usize).copied())
        };
        let (start, end) = ends(s);
        let (start, end) = (site(f.glue.start).unwrap_or(start), site(f.glue.end).unwrap_or(end));
        let (frame, flip_h, flip_v) = frame_between(start, end);
        if frame == s.frame && flip_h == s.flip_h && flip_v == s.flip_v {
            continue;
        }
        let mut moved = s.clone();
        moved.frame = frame;
        moved.flip_h = flip_h;
        moved.flip_v = flip_v;
        if let Some(f) = moved.floating.as_mut() {
            f.offset_x = frame.x;
            f.offset_y = frame.y;
        }
        out.push((bi, ri, moved));
    }
    out
}

/// The commands that make the connectors of a slide follow shapes moved to `moved` (shape, new frame), as the window does when it moves them.
pub fn follow_commands(slide: &crate::model::Slide, moved: &[(crate::model::ShapeId, crate::geometry::EmuRect)]) -> Vec<crate::command::Command> {
    let updated: Vec<Shape> = slide.shapes.iter().map(|s| match moved.iter().find(|(id, _)| *id == s.id) {
        Some((_, frame)) => Shape { frame: *frame, ..s.clone() },
        None => s.clone(),
    }).collect();
    rerouted_on_slide(&updated, &slide.shape_glues)
        .into_iter()
        .map(|(shape, frame, flip_h, flip_v)| crate::command::Command::SetConnectorFrame { slide: slide.id, shape, frame, flip_h, flip_v })
        .collect()
}

/// The connectors on a slide that must change to follow the shapes they are glued to:
/// returns list of (connector_shape_id, new_frame, new_flip_h, new_flip_v).
pub fn rerouted_on_slide(
    shapes: &[Shape],
    glues: &std::collections::HashMap<crate::model::ShapeId, crate::model::Glue>,
) -> Vec<(crate::model::ShapeId, crate::geometry::EmuRect, bool, bool)> {
    let mut out = Vec::new();
    for s in shapes {
        let Some(glue) = glues.get(&s.id) else { continue };
        if !is_connector(s) || !on_page(s) || (glue.start.is_none() && glue.end.is_none()) {
            continue;
        }
        let site = |end: Option<(u64, u32)>| -> Option<(Emu, Emu)> {
            let (id, idx) = end?;
            shapes.iter().find(|t| t.id.0 == id).and_then(|t| sites(t).get(idx as usize).copied())
        };
        let (start, end) = ends(s);
        let (start, end) = (site(glue.start).unwrap_or(start), site(glue.end).unwrap_or(end));
        let (frame, flip_h, flip_v) = frame_between(start, end);
        if frame == s.frame && flip_h == s.flip_h && flip_v == s.flip_v {
            continue;
        }
        out.push((s.id, frame, flip_h, flip_v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowParagraph, FlowRun, FlowRunStyle};
    use crate::geometry::EmuRect;
    use crate::model::{AutoShape, FloatingAnchor, Glue, ShapeId};

    fn shape(id: u64, frame: EmuRect, kind: ShapeKind) -> Shape {
        let floating = FloatingAnchor { relative_h: "page".into(), relative_v: "page".into(), offset_x: frame.x, offset_y: frame.y, ..FloatingAnchor::default() };
        Shape { id: ShapeId(id), frame, rotation: 0, flip_h: false, flip_v: false, kind, floating: Some(floating) }
    }

    fn rect(id: u64, x: Emu, y: Emu) -> Shape {
        shape(id, EmuRect::new(x, y, 100, 60), ShapeKind::Auto(AutoShape { geometry: crate::Geometry::Rect, fill: crate::drawing::Fill::None, stroke: None, text: None, tilt: None, shadow: None, glow: None, inner_shadow: None, soft_edge: None, reflection: None, bevel: None }))
    }

    fn doc_of(shapes: Vec<Shape>) -> FlowDocument {
        let mut p = FlowParagraph { id: 1, ..FlowParagraph::default() };
        for s in shapes {
            p.runs.push(FlowRun::new(FlowRunKind::Shape(Box::new(s)), FlowRunStyle::default()));
        }
        FlowDocument { body: vec![Block::Paragraph(p)], ..FlowDocument::default() }
    }

    #[test]
    fn sites_are_numbered_as_word_does() {
        let r = rect(5, 1000, 2000);
        assert_eq!(sites(&r), vec![(1050, 2000), (1000, 2030), (1050, 2060), (1100, 2030)]);
        let nearest = nearest_site(&doc_of(vec![r]), 1103, 2031, 10).unwrap();
        assert_eq!((nearest.0, nearest.1), (5, 3), "the right-hand site");
        assert!(nearest_site(&doc_of(vec![rect(5, 1000, 2000)]), 5000, 5000, 10).is_none());
    }

    #[test]
    fn a_connector_follows_the_shape_it_is_glued_to() {
        let a = rect(5, 0, 0);
        let b = rect(6, 400, 300);
        // From a's right (3) to b's left (1): (100, 30) to (400, 330).
        let mut c = shape(7, EmuRect::new(100, 30, 300, 300), ShapeKind::Line(crate::drawing::Stroke::solid(crate::drawing::Color::opaque(crate::drawing::Rgb(0)), 12_700)));
        c.floating.as_mut().unwrap().glue = Glue { start: Some((5, 3)), end: Some((6, 1)) };
        assert!(rerouted(&doc_of(vec![a.clone(), b.clone(), c.clone()])).is_empty(), "already there");
        // b moves to the left and up: the end is now at (200, 130) - 100 left, 200 up of where it was.
        let b2 = rect(6, 200, 100);
        let moved = rerouted(&doc_of(vec![a, b2, c]));
        assert_eq!(moved.len(), 1);
        let (_, _, m) = &moved[0];
        assert_eq!((m.frame.x, m.frame.y, m.frame.width, m.frame.height), (100, 30, 100, 100));
        assert!(!m.flip_h && !m.flip_v);
    }

    #[test]
    fn a_connector_running_up_and_left_is_flipped() {
        let (frame, fh, fv) = frame_between((400, 300), (100, 30));
        assert_eq!((frame.x, frame.y, frame.width, frame.height, fh, fv), (100, 30, 300, 270, true, true));
        let mut s = rect(1, 0, 0);
        s.frame = frame;
        s.flip_h = fh;
        s.flip_v = fv;
        assert_eq!(ends(&s), ((400, 300), (100, 30)));
    }

    #[test]
    fn a_slide_connector_follows_the_shape_it_is_glued_to() {
        let mut a = rect(10, 0, 0);
        a.floating = None; // A shape directly on a slide has no floating anchor
        let mut b = rect(20, 400, 300);
        b.floating = None;
        let mut c = shape(30, EmuRect::new(100, 30, 300, 300), ShapeKind::Line(crate::drawing::Stroke::solid(crate::drawing::Color::opaque(crate::drawing::Rgb(0)), 12_700)));
        c.floating = None;

        let nearest = nearest_site_on_slide(&[a.clone(), b.clone()], 102, 31, 10).unwrap();
        assert_eq!((nearest.0, nearest.1), (10, 3), "right-hand site of a");

        let mut glues = std::collections::HashMap::new();
        glues.insert(ShapeId(30), Glue { start: Some((10, 3)), end: Some((20, 1)) });

        assert!(rerouted_on_slide(&[a.clone(), b.clone(), c.clone()], &glues).is_empty(), "already connected");

        // Move b to (200, 100)
        let b2 = Shape { frame: EmuRect::new(200, 100, 100, 60), ..b };
        let moved = rerouted_on_slide(&[a, b2, c], &glues);
        assert_eq!(moved.len(), 1);
        let (cid, frame, fh, fv) = moved[0];
        assert_eq!(cid, ShapeId(30));
        assert_eq!((frame.x, frame.y, frame.width, frame.height), (100, 30, 100, 100));
        assert!(!fh && !fv);
    }
}
