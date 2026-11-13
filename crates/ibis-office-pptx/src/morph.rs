// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 変形 (Morph, 画面切り替え): the shapes the two slides have in common move,
//! resize and turn from where they were to where they are, the others fade out
//! and in. Shapes are paired by name (`!!name`, PowerPoint's way to say which
//! is which), else by what they hold: their text, their picture, their
//! geometry. 単語・文字単位 is played as by object.
use std::collections::HashMap;

use gpui_kit::*;
use ibis_doc_model::{Color, EmuRect, Fill, Geometry, Rgb, Shape, ShapeKind, Slide};

use crate::render::shape_view::{shape_element, slide_backdrop};
use crate::shell::editor::SlideEditor;

/// Which shapes of the new slide are which of the old one.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Pairing {
    /// (index in the old slide's shown shapes, index in the new one's).
    pub(crate) pairs: Vec<(usize, usize)>,
    pub(crate) old_only: Vec<usize>,
    pub(crate) new_only: Vec<usize>,
}

fn preset(geometry: &Geometry) -> String {
    match geometry {
        Geometry::Rect => "rect".into(),
        Geometry::RoundRect { .. } => "roundRect".into(),
        Geometry::Ellipse => "ellipse".into(),
        Geometry::Formula(f) => f.preset.clone().unwrap_or_else(|| "custom".into()),
    }
}

/// What makes a shape the same as another one: `None` for what is not paired.
fn key(slide: &Slide, shape: &Shape) -> Option<String> {
    if let Some(name) = slide.shape_names.get(&shape.id).filter(|n| n.starts_with("!!")) {
        return Some(format!("name:{name}"));
    }
    match &shape.kind {
        ShapeKind::Auto(auto) => {
            let text = auto.text.as_ref().map(|t| t.text()).unwrap_or_default();
            Some(if text.trim().is_empty() { format!("auto:{}", preset(&auto.geometry)) } else { format!("auto:{}:{text}", preset(&auto.geometry)) })
        }
        ShapeKind::TextBox(body) => Some(format!("text:{}", body.text())),
        ShapeKind::Picture(p) => Some(format!("pic:{:?}", p.media)),
        _ => None,
    }
}

fn text_of(shape: &Shape) -> Option<String> {
    match &shape.kind {
        ShapeKind::Auto(auto) => auto.text.as_ref().map(|t| t.text()),
        ShapeKind::TextBox(body) => Some(body.text()),
        _ => None,
    }
}

fn word_overlap_score(s1: &str, s2: &str) -> usize {
    let w1: std::collections::HashSet<&str> = s1.split_whitespace().filter(|w| !w.is_empty()).collect();
    let w2: std::collections::HashSet<&str> = s2.split_whitespace().filter(|w| !w.is_empty()).collect();
    w1.intersection(&w2).count()
}

fn char_overlap_score(s1: &str, s2: &str) -> usize {
    let c1: std::collections::HashSet<char> = s1.chars().filter(|c| !c.is_whitespace()).collect();
    let c2: std::collections::HashSet<char> = s2.chars().filter(|c| !c.is_whitespace()).collect();
    c1.intersection(&c2).count()
}

/// The shapes of `new` that are those of `old`, in order: each of the old
/// ones taken once.
pub(crate) fn pair_shapes(old: &Slide, new: &Slide) -> Pairing {
    let olds: Vec<&Shape> = old.shown_shapes().collect();
    let news: Vec<&Shape> = new.shown_shapes().collect();
    let old_keys: Vec<Option<String>> = olds.iter().map(|s| key(old, s)).collect();
    let mut taken = vec![false; olds.len()];
    let mut pairs = Vec::new();
    let mut new_only = Vec::new();
    // Named ones first (they say which is which), then by what they hold.
    let mut matched: HashMap<usize, usize> = HashMap::new();
    for pass in 0..2 {
        for (j, shape) in news.iter().enumerate() {
            if matched.contains_key(&j) {
                continue;
            }
            let Some(k) = key(new, shape) else { continue };
            if (pass == 0) != k.starts_with("name:") {
                continue;
            }
            if let Some(i) = (0..olds.len()).find(|&i| !taken[i] && old_keys[i].as_deref() == Some(k.as_str())) {
                taken[i] = true;
                matched.insert(j, i);
            }
        }
    }
    // For byWord or byChar morph transitions, pair remaining text shapes by overlapping words or characters.
    let option = new.transition.as_ref().and_then(|t| t.direction.as_deref()).unwrap_or("byObject");
    if option == "byWord" || option == "byChar" {
        for (j, shape) in news.iter().enumerate() {
            if matched.contains_key(&j) {
                continue;
            }
            let Some(new_text) = text_of(shape) else { continue };
            let mut best_i = None;
            let mut best_score = 0;
            for (i, old_shape) in olds.iter().enumerate() {
                if taken[i] {
                    continue;
                }
                let Some(old_text) = text_of(old_shape) else { continue };
                let score = if option == "byWord" {
                    word_overlap_score(&old_text, &new_text)
                } else {
                    char_overlap_score(&old_text, &new_text)
                };
                if score > best_score {
                    best_score = score;
                    best_i = Some(i);
                }
            }
            if let Some(i) = best_i {
                taken[i] = true;
                matched.insert(j, i);
            }
        }
    }
    for j in 0..news.len() {
        match matched.get(&j) {
            Some(&i) => pairs.push((i, j)),
            None => new_only.push(j),
        }
    }
    let old_only = (0..olds.len()).filter(|&i| !taken[i]).collect();
    Pairing { pairs, old_only, new_only }
}

/// The frame and turn of a shape part of the way (`t`, 0 to 1) from `a` to `b`.
pub(crate) fn between(a: &Shape, b: &Shape, t: f64) -> (EmuRect, i32) {
    let lerp = |x: i64, y: i64| (x as f64 + (y - x) as f64 * t).round() as i64;
    let frame = EmuRect::new(lerp(a.frame.x, b.frame.x), lerp(a.frame.y, b.frame.y), lerp(a.frame.width, b.frame.width), lerp(a.frame.height, b.frame.height));
    // The short way round.
    const FULL: i64 = 21_600_000;
    let mut delta = (b.rotation as i64 - a.rotation as i64).rem_euclid(FULL);
    if delta > FULL / 2 {
        delta -= FULL;
    }
    (frame, (a.rotation as f64 + delta as f64 * t).round().rem_euclid(FULL as f64) as i32)
}

/// A colour part of the way (`t`, 0 to 1) from `a` to `b`, each channel (and the opacity) on its own.
fn lerp_color(a: Color, b: Color, t: f64) -> Color {
    let channel = |x: u32, y: u32| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round().clamp(0.0, 255.0) as u32;
    let (Rgb(x), Rgb(y)) = (a.rgb, b.rgb);
    let rgb = channel((x >> 16) & 0xff, (y >> 16) & 0xff) << 16 | channel((x >> 8) & 0xff, (y >> 8) & 0xff) << 8 | channel(x & 0xff, y & 0xff);
    Color { rgb: Rgb(rgb), alpha: channel(u32::from(a.alpha), u32::from(b.alpha)) as u8 }
}

/// The shape `b` with the colours (fill, outline and its width, text) part of the way from those of `a`, when the two are
/// the same figure with the same words and only their looks differ (what PowerPoint's morph shows as a colour that changes
/// smoothly); `None` when they differ in more than that (they are then faded one into the other).
pub(crate) fn blend(a: &Shape, b: &Shape, t: f64) -> Option<Shape> {
    let (ShapeKind::Auto(x), ShapeKind::Auto(y)) = (&a.kind, &b.kind) else { return None };
    if x.geometry != y.geometry || x.stroke.is_some() != y.stroke.is_some() {
        return None;
    }
    let fill = match (&x.fill, &y.fill) {
        (Fill::None, Fill::None) => Fill::None,
        (Fill::Solid(p), Fill::Solid(q)) => Fill::Solid(lerp_color(*p, *q, t)),
        _ => return None,
    };
    let (xt, yt) = (x.text.as_ref(), y.text.as_ref());
    let same_words = match (xt, yt) {
        (None, None) => true,
        (Some(p), Some(q)) => p.paragraphs.len() == q.paragraphs.len() && p.paragraphs.iter().zip(&q.paragraphs).all(|(m, n)| m.runs.len() == n.runs.len() && m.runs.iter().zip(&n.runs).all(|(r, s)| r.text == s.text)),
        _ => false,
    };
    if !same_words {
        return None;
    }
    let mut out = b.clone();
    let ShapeKind::Auto(blended) = &mut out.kind else { return None };
    blended.fill = fill;
    if let (Some(p), Some(q), Some(line)) = (&x.stroke, &y.stroke, blended.stroke.as_mut()) {
        line.color = lerp_color(p.color, q.color, t);
        line.width = (p.width as f64 + (q.width - p.width) as f64 * t).round() as _;
    }
    if let (Some(p), Some(body)) = (xt, blended.text.as_mut()) {
        for (para, from) in body.paragraphs.iter_mut().zip(&p.paragraphs) {
            for (run, was) in para.runs.iter_mut().zip(&from.runs) {
                if let (Some(now), Some(before)) = (run.style.color, was.style.color) {
                    run.style.color = Some(lerp_color(Color::opaque(before), Color::opaque(now), t).rgb);
                }
            }
        }
    }
    Some(out)
}

#[ibis_office_macros::editor_ext(Morph)]
impl SlideEditor {
    /// The slide `slide` coming in from `before` as a morph, `t` (0 to 1) of the way.
    pub(crate) fn morph_page(&self, before: &Slide, slide: &Slide, t: f32, scale: f32) -> AnyElement {
        let pairing = pair_shapes(before, slide);
        let olds: Vec<&Shape> = before.shown_shapes().collect();
        let news: Vec<&Shape> = slide.shown_shapes().collect();
        let layer = |opacity: f32, element: AnyElement| div().absolute().top_0().left_0().size_full().opacity(opacity).child(element).into_any_element();
        let mut page = div().absolute().top_0().left_0().size_full().overflow_hidden().bg(crate::shell::palette::white());
        page = page.child(slide_backdrop(before, scale, &self.resources)).child(layer(t, slide_backdrop(slide, scale, &self.resources).into_any_element()));
        for &i in &pairing.old_only {
            page = page.child(layer(1.0 - t, shape_element(olds[i], olds[i].frame, scale, &self.resources, None)));
        }
        for &(i, j) in &pairing.pairs {
            let (frame, rotation) = between(olds[i], news[j], t as f64);
            if let Some(mut mixed) = blend(olds[i], news[j], t as f64) {
                mixed.rotation = rotation;
                page = page.child(layer(1.0, shape_element(&mixed, frame, scale, &self.resources, None)));
                continue;
            }
            let mut old = olds[i].clone();
            old.rotation = rotation;
            let mut new = news[j].clone();
            new.rotation = rotation;
            page = page.child(layer(1.0 - t, shape_element(&old, frame, scale, &self.resources, None))).child(layer(t, shape_element(&new, frame, scale, &self.resources, None)));
        }
        for &j in &pairing.new_only {
            page = page.child(layer(t, shape_element(news[j], news[j].frame, scale, &self.resources, None)));
        }
        page.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{between, blend, pair_shapes, Pairing};
    use ibis_doc_model::{AutoShape, EmuRect, Fill, Geometry, Shape, ShapeId, ShapeKind, Slide, SlideId, TextBody};

    fn shape(id: u64, frame: EmuRect, geometry: Geometry, text: &str) -> Shape {
        let mut auto = AutoShape::new(geometry, Fill::None, None);
        if !text.is_empty() {
            auto.text = Some(TextBody::plain(text, Default::default()));
        }
        Shape::new(ShapeId(id), frame, ShapeKind::Auto(auto))
    }

    fn slide(shapes: Vec<Shape>) -> Slide {
        let mut s = Slide::new(SlideId(1));
        s.shapes = shapes;
        s
    }

    #[test]
    fn shapes_are_paired_by_what_they_hold() {
        let f = EmuRect::new(0, 0, 100, 100);
        let old = slide(vec![shape(1, f, Geometry::Rect, "A"), shape(2, f, Geometry::Ellipse, ""), shape(3, f, Geometry::Rect, "gone")]);
        let new = slide(vec![shape(4, f, Geometry::Ellipse, ""), shape(5, f, Geometry::Rect, "A"), shape(6, f, Geometry::Rect, "new")]);
        assert_eq!(pair_shapes(&old, &new), Pairing { pairs: vec![(1, 0), (0, 1)], old_only: vec![2], new_only: vec![2] });
        // Named shapes pair by name, whatever they hold.
        let mut old = slide(vec![shape(1, f, Geometry::Rect, "one"), shape(2, f, Geometry::Rect, "two")]);
        let mut new = slide(vec![shape(3, f, Geometry::Ellipse, "x"), shape(4, f, Geometry::Rect, "two")]);
        old.shape_names.insert(ShapeId(1), "!!title".into());
        new.shape_names.insert(ShapeId(3), "!!title".into());
        assert_eq!(pair_shapes(&old, &new), Pairing { pairs: vec![(0, 0), (1, 1)], old_only: vec![], new_only: vec![] });
        // Each old shape once.
        let old = slide(vec![shape(1, f, Geometry::Rect, "A")]);
        let new = slide(vec![shape(2, f, Geometry::Rect, "A"), shape(3, f, Geometry::Rect, "A")]);
        assert_eq!(pair_shapes(&old, &new).pairs, vec![(0, 0)]);
    }

    #[test]
    fn colours_change_smoothly_between_the_same_figure() {
        use ibis_doc_model::{Color, Rgb, Stroke};
        let f = EmuRect::new(0, 0, 100, 100);
        let paint = |shape: &mut Shape, fill: u32, line: u32, width: i64| {
            if let ShapeKind::Auto(a) = &mut shape.kind {
                a.fill = Fill::Solid(Color::opaque(Rgb(fill)));
                a.stroke = Some(Stroke::solid(Color::opaque(Rgb(line)), width));
            }
        };
        let mut a = shape(1, f, Geometry::Rect, "A");
        let mut b = shape(2, f, Geometry::Rect, "A");
        paint(&mut a, 0x000000, 0xff0000, 100);
        paint(&mut b, 0xff8040, 0x0000ff, 300);
        let mixed = blend(&a, &b, 0.5).expect("the same figure and words");
        let ShapeKind::Auto(m) = &mixed.kind else { panic!() };
        assert_eq!(m.fill, Fill::Solid(Color::opaque(Rgb(0x804020))));
        let line = m.stroke.as_ref().unwrap();
        assert_eq!((line.color, line.width), (Color::opaque(Rgb(0x800080)), 200));
        // At the ends it is the one or the other.
        assert_eq!(blend(&a, &b, 0.0).map(|s| s.kind), Some({ let mut s = b.clone(); if let ShapeKind::Auto(x) = &mut s.kind { x.fill = Fill::Solid(Color::opaque(Rgb(0))); x.stroke.as_mut().unwrap().color = Color::opaque(Rgb(0xff0000)); x.stroke.as_mut().unwrap().width = 100; } s.kind }));
        // Other words or another figure are faded, not blended.
        assert!(blend(&a, &shape(3, f, Geometry::Rect, "B"), 0.5).is_none());
        assert!(blend(&a, &shape(4, f, Geometry::Ellipse, "A"), 0.5).is_none());
    }

    #[test]
    fn halfway() {
        let mut a = shape(1, EmuRect::new(0, 0, 100, 100), Geometry::Rect, "");
        let mut b = shape(2, EmuRect::new(200, 100, 300, 100), Geometry::Rect, "");
        a.rotation = 21_600_000 - 600_000;
        b.rotation = 600_000;
        let (frame, rotation) = between(&a, &b, 0.5);
        assert_eq!(frame, EmuRect::new(100, 50, 200, 100));
        // The short way round: through 0, not the long way.
        assert_eq!(rotation, 0);
        assert_eq!(between(&a, &b, 0.0).0, a.frame);
        assert_eq!(between(&a, &b, 1.0).0, b.frame);
    }

    #[test]
    fn shapes_are_paired_by_word_and_char_options() {
        use ibis_doc_model::Transition;
        let f = EmuRect::new(0, 0, 100, 100);

        // Under byObject: different text does not pair
        let old = slide(vec![shape(1, f, Geometry::Rect, "Annual Report 2025")]);
        let mut new = slide(vec![shape(2, f, Geometry::Rect, "Annual Report 2026")]);
        assert_eq!(pair_shapes(&old, &new), Pairing { pairs: vec![], old_only: vec![0], new_only: vec![0] });

        // Under byWord: common words ("Annual", "Report") cause them to pair
        new.transition = Some(Transition { direction: Some("byWord".into()), ..Transition::new("morph") });
        assert_eq!(pair_shapes(&old, &new), Pairing { pairs: vec![(0, 0)], old_only: vec![], new_only: vec![] });

        // Under byWord: no common words -> no match
        let old_chars = slide(vec![shape(1, f, Geometry::Rect, "AlphaBeta")]);
        let mut new_chars = slide(vec![shape(2, f, Geometry::Rect, "BetaGamma")]);
        new_chars.transition = Some(Transition { direction: Some("byWord".into()), ..Transition::new("morph") });
        assert_eq!(pair_shapes(&old_chars, &new_chars), Pairing { pairs: vec![], old_only: vec![0], new_only: vec![0] });

        // Under byChar: common characters ("Beta") cause them to pair
        new_chars.transition = Some(Transition { direction: Some("byChar".into()), ..Transition::new("morph") });
        assert_eq!(pair_shapes(&old_chars, &new_chars), Pairing { pairs: vec![(0, 0)], old_only: vec![], new_only: vec![] });
    }
}
