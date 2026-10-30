// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Typesetting and drawing a math formula ([`Math`]), for every format.
//!
//! [`typeset`] lays a formula out as a [`MathBox`]: its width, how far it
//! reaches above and below its baseline, and the pieces to draw (text and
//! rules), all in pixels at a font size. It measures text through a
//! closure, so the layout needs no window. [`paint`] draws a box.
//!
//! The spacing and sizes are plain typographic proportions of the font
//! size (the markup, ECMA-376 22.1, names the parts but gives no numbers).

use gpui_kit::*;
use ibis_doc_model::math::{FracKind, Math};

/// One thing to draw. `x` is from the box's left, `y` from its baseline
/// (down is positive).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Piece {
    Text { x: f32, y: f32, text: String, size: f32, italic: bool },
    /// A filled rectangle.
    Rule { x: f32, y: f32, w: f32, h: f32 },
    /// A straight line of a thickness.
    Line { x0: f32, y0: f32, x1: f32, y1: f32, t: f32 },
}

/// A typeset formula.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MathBox {
    pub(crate) width: f32,
    /// Above the baseline.
    pub(crate) ascent: f32,
    /// Below the baseline.
    pub(crate) descent: f32,
    pub(crate) pieces: Vec<Piece>,
}

impl MathBox {
    fn shifted(mut self, dx: f32, dy: f32) -> Vec<Piece> {
        for p in &mut self.pieces {
            match p {
                Piece::Text { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
                Piece::Rule { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
                Piece::Line { x0, y0, x1, y1, .. } => {
                    *x0 += dx;
                    *x1 += dx;
                    *y0 += dy;
                    *y1 += dy;
                }
            }
        }
        self.pieces
    }

    fn put(&mut self, other: MathBox, dx: f32, dy: f32) {
        self.ascent = self.ascent.max(other.ascent - dy);
        self.descent = self.descent.max(other.descent + dy);
        self.width = self.width.max(dx + other.width);
        self.pieces.extend(other.shifted(dx, dy));
    }
}

/// A text's width, ascent and descent at a size (pixels), upright or italic.
pub(crate) type Measure<'a> = &'a mut dyn FnMut(&str, f32, bool) -> (f32, f32, f32);

// The proportions of the font size come from the maths font's MATH table (`math_font`). These are not in it: the space round a
// binary operator and a relation (TeX's medium and thick spaces are 4/18 and 5/18 of an em; the first is a little more
// here, as PowerPoint draws it), and the sizes of a matrix and of a stretched square bracket, measured on PowerPoint's.
/// The space on each side of a binary operator.
const OP_PAD: f32 = 0.26;
/// The space on each side of a relation (=, <, →…).
const REL_PAD: f32 = 0.278;

fn mx() -> &'static crate::render::math_font::MathMetrics {
    crate::render::math_font::metrics()
}

thread_local! {
    /// Whether the formula being laid out is in script size (a superscript, a subscript, a limit): operators have no space
    /// round them there.
    static SCRIPTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Whether the formula being laid out is in cramped style (under a bar or a root, or in a subscript): its superscripts
    /// are raised less.
    static CRAMPED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `f` with script size on.
fn scripted<T>(f: impl FnOnce() -> T) -> T {
    let before = SCRIPTED.with(|c| c.replace(true));
    let out = f();
    SCRIPTED.with(|c| c.set(before));
    out
}

/// `f` with cramped style on.
fn cramped<T>(f: impl FnOnce() -> T) -> T {
    let before = CRAMPED.with(|c| c.replace(true));
    let out = f();
    CRAMPED.with(|c| c.set(before));
    out
}

/// A combining mark or the like, drawn over or under its base as an accent.
fn is_accent_char(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36F | 0x20D0..=0x20FF | 0x2DC | 0x2C6 | 0x2C7 | 0x2D9 | 0xA8 | 0xB4 | 0x60 | 0xAF | 0x2192 | 0x2190 | 0x2194) || "⏜⏝⏞⏟⎴⎵".contains(c)
}

/// Names of functions: upright, with a thin space before their argument.
const FUNCTIONS: &[&str] = &["sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "coth", "log", "ln", "lg", "exp", "lim", "liminf", "limsup", "max", "min", "sup", "inf", "det", "gcd", "dim", "ker", "deg", "arg", "mod", "Pr", "hom"];

fn is_operator(c: char) -> bool {
    matches!(c, '+' | '−' | '-' | '±' | '∓' | '×' | '÷' | '·' | '=' | '≠' | '≈' | '≤' | '≥' | '<' | '>' | '→' | '←' | '↔' | '⇒' | '⇔' | '∈' | '∉' | '⊂' | '⊆' | '∪' | '∩' | '∝' | '≡')
}

/// Lays `math` out at `size` pixels.
pub(crate) fn typeset(math: &Math, size: f32, measure: Measure) -> MathBox {
    let mut boxed = lay(math, size, false, measure);
    // The line of a formula is at least as high as the maths font's, and the font's leading higher than its highest part.
    boxed.ascent = (boxed.ascent + size * mx().leading).max(size * mx().ascent);
    boxed.descent = boxed.descent.max(size * mx().descent);
    boxed
}

/// How far the ink of `text` reaches above and below its baseline (of its size), as the maths font draws it. `None`: the
/// font has no glyph for some character, and the font's own ascent and descent stand.
fn ink_of(text: &str) -> Option<(f32, f32)> {
    let mut ascent = 0.0f32;
    let mut descent = 0.0f32;
    for c in text.chars().filter(|c| *c != ' ') {
        let (below, above) = mx().ink(c)?;
        ascent = ascent.max(above);
        descent = descent.max(below);
    }
    Some((ascent, descent))
}

fn text_box(text: &str, size: f32, italic: bool, measure: Measure) -> MathBox {
    text_box_as(text, text, size, italic, measure)
}

/// `shown` drawn, its ink taken from what `ink` is.
fn text_box_as(shown: &str, ink: &str, size: f32, italic: bool, measure: Measure) -> MathBox {
    let (w, mut a, mut d) = measure(shown, size, italic);
    let _ = ink;
    if let Some((ink_a, ink_d)) = ink_of(shown) {
        (a, d) = (ink_a * size, ink_d * size);
    }
    MathBox { width: w, ascent: a, descent: d, pieces: vec![Piece::Text { x: 0.0, y: 0.0, text: shown.to_string(), size, italic }] }
}

/// The italic letter `c` as the Unicode math italic (𝑥, 𝛼), which maths fonts draw with their own italic shapes and
/// spacing (what OMML stores for an italic variable); other characters stay as they are.
fn math_italic(c: char) -> char {
    let at = |base: u32, from: char| char::from_u32(base + (c as u32 - from as u32)).unwrap_or(c);
    match c {
        'h' => '\u{210E}',
        'a'..='z' => at(0x1D44E, 'a'),
        'A'..='Z' => at(0x1D434, 'A'),
        'α'..='ω' => at(0x1D6FC, 'α'),
        _ => c,
    }
}

fn lay(math: &Math, size: f32, upright: bool, m: Measure) -> MathBox {
    match math {
        Math::Text(t) => lay_text(t, size, upright || FUNCTIONS.contains(&t.as_str()), m, false),
        Math::Row(items) => {
            // A leading + − ± of a text is a sign unless something comes before it (then it is a binary operator).
            let mut laid = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                let after_operand = i > 0 && !matches!(&items[i - 1], Math::Text(t) if t.chars().last().is_some_and(is_operator));
                laid.push(match item {
                    Math::Text(t) => lay_text(t, size, upright, m, after_operand),
                    other => lay(other, size, upright, m),
                });
            }
            lay_row(laid, size)
        }
        Math::Frac { num, den, kind } => match kind {
            FracKind::Linear => {
                let (n, d) = (lay(num, size, upright, m), lay(den, size, upright, m));
                lay_row(vec![n, text_box("/", size, false, m), d], size)
            }
            FracKind::Skew => {
                // The numerator above and before the slash, the denominator below and after it.
                let small = size * 0.85;
                let (n, d) = (lay(num, small, upright, m), lay(den, small, upright, m));
                let slash_w = size * 0.35;
                let mut out = MathBox::default();
                let (nw, dw) = (n.width, d.width);
                out.put(n, 0.0, -size * 0.4);
                out.pieces.push(Piece::Line { x0: nw + size * 0.02, y0: size * 0.3, x1: nw + size * 0.02 + slash_w, y1: -size * 1.0, t: size * mx().rule });
                out.ascent = out.ascent.max(size * 1.0);
                out.descent = out.descent.max(size * 0.3);
                out.put(d, nw + slash_w * 0.8, size * 0.45);
                out.width = nw + slash_w * 0.8 + dw;
                out
            }
            FracKind::Bar | FracKind::NoBar => {
                let bar = *kind == FracKind::Bar;
                let (n, d) = (lay(num, size, upright, m), cramped(|| lay(den, size, upright, m)));
                let w = n.width.max(d.width);
                let thick = if bar { (size * mx().rule).max(1.0) } else { 0.0 };
                let axis = size * mx().axis;
                let gap = size * mx().frac_gap;
                // Each part sits where the font says (display style), further away if it would come nearer than the least gap.
                let up = (size * mx().num_up).max(axis + thick / 2.0 + gap + n.descent);
                let down = (size * mx().den_down).max(thick / 2.0 - axis + gap + d.ascent);
                let mut out = MathBox { width: w, ..Default::default() };
                if bar {
                    out.pieces.push(Piece::Rule { x: 0.0, y: -axis - thick / 2.0, w, h: thick });
                }
                out.ascent = axis + thick / 2.0;
                out.descent = thick / 2.0 - axis;
                let (nw, dw) = (n.width, d.width);
                out.put(n, (w - nw) / 2.0, -up);
                out.put(d, (w - dw) / 2.0, down);
                out.width = w;
                out
            }
        },
        Math::Script { base, sub, sup } => {
            let glyph_base = matches!(&**base, Math::Text(t) if t.chars().count() == 1);
            let b = lay(base, size, upright, m);
            let small = size * mx().script;
            let bw = b.width;
            let basc = b.ascent;
            let bdesc = b.descent;
            let mut out = MathBox::default();
            out.put(b, 0.0, 0.0);
            let was_cramped = CRAMPED.with(std::cell::Cell::get);
            let sup_box = sup.as_ref().map(|s| scripted(|| lay(s, small, upright, m)));
            let sub_box = sub.as_ref().map(|s| scripted(|| cramped(|| lay(s, small, upright, m))));
            // How far the baselines of the scripts are from the base's. A single glyph as the base is not measured for its
            // height (as in TeX); a bigger base holds the script away from its edge.
            let mut up = sup_box.as_ref().map(|s| {
                let shift = size * if was_cramped { mx().sup_up_cramped } else { mx().sup_up };
                let mut u = shift.max(s.descent + size * mx().sup_bottom_min);
                if !glyph_base {
                    u = u.max(basc - small * mx().sup_drop);
                }
                u
            });
            let mut down = sub_box.as_ref().map(|s| {
                let mut v = (size * mx().sub_down).max(s.ascent - size * mx().sub_top_max);
                if !glyph_base {
                    v = v.max(bdesc + small * mx().sub_drop);
                }
                v
            });
            // Both: the scripts are kept apart by at least the gap the font names.
            if let (Some(u), Some(v), Some(sp), Some(sb)) = (up, down, sup_box.as_ref(), sub_box.as_ref()) {
                let gap = (u - sp.descent) - (sb.ascent - v);
                let least = size * mx().sub_sup_gap;
                if gap < least {
                    down = Some(v + (least - gap));
                    up = Some(u);
                }
            }
            let kern = size * mx().script_space;
            let mut x_end = bw;
            if let (Some(s), Some(u)) = (sup_box, up) {
                x_end = x_end.max(bw + s.width + kern);
                out.put(s, bw, -u);
            }
            if let (Some(s), Some(v)) = (sub_box, down) {
                x_end = x_end.max(bw + s.width + kern);
                out.put(s, bw, v);
            }
            out.width = x_end;
            out
        }
        Math::Radical { degree, body } => {
            let b = cramped(|| lay(body, size, upright, m));
            let bar = (size * mx().rad_rule).max(1.0);
            let pad = 0.0;
            let sign_w = size * mx().advance('√').unwrap_or(0.6);
            let top = b.ascent + size * mx().rad_gap + bar;
            let deg = degree.as_ref().map(|d| lay(d, size * 0.55, upright, m));
            let deg_w = deg.as_ref().map_or(0.0, |d| d.width);
            let x0 = (deg_w - size * 0.2).max(0.0);
            let mut out = MathBox::default();
            // The tick, the long stroke, and the bar over the body.
            let bottom = b.descent + size * 0.05;
            out.pieces.push(Piece::Line { x0, y0: -top * 0.45, x1: x0 + sign_w * 0.2, y1: -top * 0.5, t: bar });
            out.pieces.push(Piece::Line { x0: x0 + sign_w * 0.2, y0: -top * 0.5, x1: x0 + sign_w * 0.5, y1: bottom, t: bar * 1.5 });
            out.pieces.push(Piece::Line { x0: x0 + sign_w * 0.5, y0: bottom, x1: x0 + sign_w, y1: -top + bar / 2.0, t: bar });
            out.pieces.push(Piece::Rule { x: x0 + sign_w, y: -top, w: b.width + pad * 2.0, h: bar });
            out.width = x0 + sign_w;
            out.ascent = top;
            out.descent = bottom;
            let bw = b.width;
            out.put(b, x0 + sign_w + pad, 0.0);
            out.width = x0 + sign_w + pad * 2.0 + bw;
            if let Some(d) = deg {
                out.put(d, 0.0, -(top * 0.55));
            }
            out
        }
        Math::Nary { op, sub, sup, body, stacked } => {
            let integral = !ibis_doc_model::math::stacked_by_default(op);
            // The operator drawn large (the sizes of Cambria Math's display variants, as PowerPoint draws them), its ink
            // (taken from the glyph's) centred at a height above the baseline: a sum on the axis, an integral a little lower.
            // The sign drawn large, as the font grows it in display style (its glyph scaled to the variant); `centre` is where
            // its ink is centred above the baseline. Without the font's variants: twice the size, on the axis.
            let first = op.chars().next();
            let (scale, ink_below, ink_above, ink_right, centre_em) = match first.and_then(|c| mx().big_operator(c)) {
                Some(b) => (b.scale, b.below, b.above, b.right, b.centre),
                None => (2.0, 0.21, 0.7, 0.65, if integral { 0.19 } else { mx().axis }),
            };
            let big = size * scale;
            let centre = size * centre_em;
            // A double or triple integral is its integral signs side by side (the fonts seldom have the one glyph).
            let (glyph, copies) = match op.as_str() {
                "∬" => ("∫", 2),
                "∭" => ("∫", 3),
                "∯" => ("∮", 2),
                "∰" => ("∮", 3),
                other => (other, 1),
            };
            let o = if copies > 1 {
                let one = text_box(glyph, big, false, m);
                let step = one.width * 0.62;
                let mut combined = MathBox { width: step * (copies - 1) as f32 + one.width, ascent: one.ascent, descent: one.descent, pieces: Vec::new() };
                for k in 0..copies {
                    combined.pieces.extend(one.clone().shifted(step * k as f32, 0.0));
                }
                combined
            } else {
                text_box_as(op, "", big, false, m)
            };
            let small = size * mx().script;
            let (s, u) = (sub.as_ref().map(|s| scripted(|| cramped(|| lay(s, small, upright, m)))), sup.as_ref().map(|s| scripted(|| lay(s, small, upright, m))));
            let mut out = MathBox::default();
            // The glyph's baseline below the formula's, and the ink's reach above and below the formula's baseline.
            let glyph_below = (ink_above - ink_below) / 2.0 * big - centre;
            let ink_top = centre + (ink_above + ink_below) / 2.0 * big;
            let ink_bottom = (ink_above + ink_below) / 2.0 * big - centre;
            let ow = o.width;
            if *stacked {
                // Limits centred over and under the sign, at the least distances the font names.
                let w = ow.max(s.as_ref().map_or(0.0, |b| b.width)).max(u.as_ref().map_or(0.0, |b| b.width));
                out.put(o, (w - ow) / 2.0, glyph_below);
                if let Some(u) = u {
                    let uw = u.width;
                    let dy = -(ink_top + (size * mx().upper_rise).max(size * mx().upper_gap + u.descent));
                    out.put(u, (w - uw) / 2.0, dy);
                }
                if let Some(s) = s {
                    let sw = s.width;
                    let dy = ink_bottom + (size * mx().lower_drop).max(size * mx().lower_gap + s.ascent);
                    out.put(s, (w - sw) / 2.0, dy);
                }
                out.width = w;
            } else {
                // An integral: the limits at its right, the upper one at its top and the lower one at its foot.
                out.put(o, 0.0, glyph_below);
                let mut w = ow;
                if let Some(u) = u {
                    let x = big * ink_right * 0.96;
                    w = w.max(x + u.width);
                    out.put(u, x, -(ink_top - size * 0.19));
                }
                if let Some(s) = s {
                    let x = big * ink_right * 0.6;
                    w = w.max(x + s.width);
                    out.put(s, x, ink_bottom);
                }
                out.width = w;
            }
            let b = lay(body, size, upright, m);
            let x = out.width + size * if *stacked { 0.09 } else { 0.04 };
            let bw = b.width;
            out.put(b, x, 0.0);
            out.width = x + bw;
            out
        }
        Math::Delimited { open, close, sep, items } => {
            let mut inner = MathBox::default();
            let parts: Vec<MathBox> = items.iter().map(|i| lay(i, size, upright, m)).collect();
            let sep_box = if items.len() > 1 && !sep.is_empty() { Some(text_box(sep, size, false, m)) } else { None };
            let mut x = 0.0;
            for (i, p) in parts.into_iter().enumerate() {
                if i > 0 && let Some(s) = &sep_box {
                    let w = s.width;
                    inner.put(s.clone(), x + size * 0.1, 0.0);
                    x += w + size * 0.2;
                }
                let w = p.width;
                inner.put(p, x, 0.0);
                x += w;
            }
            inner.width = x;
            brackets(inner, open, close, size, m)
        }
        Math::Function { name, arg } => {
            let n = lay(name, size, true, m);
            let a = lay(arg, size, upright, m);
            // A thin space (3/18 of the size, TeX's) between the name and what it is applied to.
            let space = MathBox { width: size * 3.0 / 18.0, ..Default::default() };
            lay_row(vec![n, space, a], size)
        }
        Math::Mark { base, mark, over } => {
            let b = lay(base, size, upright, m);
            let _small = size * 0.8;
            let w = b.width;
            let mut out = MathBox::default();
            let (ba, bd) = (b.ascent, b.descent);
            out.put(b, 0.0, 0.0);
            let overline = matches!(mark.as_str(), "\u{305}" | "\u{332}" | "‾" | "¯" | "_");
            if overline {
                let y = if *over { -(ba + size * 0.08) } else { bd + size * 0.08 };
                out.put(MathBox { width: w, pieces: vec![Piece::Rule { x: 0.0, y: 0.0, w, h: size * mx().rule }], ..Default::default() }, 0.0, y);
                out.ascent = out.ascent.max(-y + size * mx().rule);
                out.descent = out.descent.max(y + size * mx().rule);
            } else if mark.chars().count() == 1 && mark.chars().next().is_some_and(is_accent_char) {
                // An accent: the maths font's own glyph (a combining mark: it stands over its origin), centred on the base and
                // lifted when the base is higher than the font's accent base height.
                let c = mark.chars().next().unwrap_or(' ');
                // A combining mark drawn alone is not placed where its shape says: its spacing look-alike is drawn instead.
                let c = match c {
                    '\u{300}' => '`',
                    '\u{301}' => '\u{B4}',
                    '\u{302}' => '\u{2C6}',
                    '\u{303}' => '\u{2DC}',
                    '\u{304}' => '\u{AF}',
                    '\u{307}' => '\u{2D9}',
                    '\u{308}' => '\u{A8}',
                    '\u{30C}' => '\u{2C7}',
                    other => other,
                };
                if let (Some((below, above)), Some((x0, x1))) = (mx().ink(c), mx().ink_x(c)) {
                    let lift = if *over { (ba - size * mx().accent_base_height).max(0.0) } else { 0.0 };
                    let g = text_box_as(&c.to_string(), "", size, false, m);
                    let x = w / 2.0 - (x0 + x1) / 2.0 * size;
                    if *over {
                        out.put(MathBox { width: 0.0, ascent: above * size, descent: 0.0, pieces: g.pieces }, x, -lift);
                        out.ascent = out.ascent.max(lift + above * size);
                    } else {
                        let drop = bd + size * 0.05 + above * size;
                        out.put(MathBox { width: 0.0, ascent: 0.0, descent: below * size, pieces: g.pieces }, x, drop);
                        out.descent = out.descent.max(drop + below.max(0.0) * size);
                    }
                }
                out.width = out.width.max(w);
            } else {
                // A limit (`lim` over x→0): at script size, centred under or over the base, at the distances the font names.
                let limit = scripted(|| lay_text(mark, size * mx().script, upright, m, false));
                let lw = limit.width;
                let at = (w - lw) / 2.0;
                if at < 0.0 {
                    // The limit is wider than the base: the base moves right to be centred over it.
                    let moved = std::mem::take(&mut out);
                    out = MathBox { width: moved.width, ascent: moved.ascent, descent: moved.descent, pieces: moved.shifted(-at, 0.0) };
                }
                if *over {
                    let dy = -((size * mx().upper_rise).max(ba + size * mx().upper_gap + limit.descent));
                    out.put(limit, at.max(0.0), dy);
                } else {
                    let dy = (size * mx().lower_drop).max(bd + size * mx().lower_gap + limit.ascent);
                    out.put(limit, at.max(0.0), dy);
                }
                out.width = out.width.max(w).max(lw);
            }
            out
        }
        Math::EqArray(rows) => {
            // Each row split where its `&` is: the left parts end at one place, the right parts begin there.
            let parts: Vec<(MathBox, MathBox)> = rows
                .iter()
                .map(|r| {
                    let (left, right) = split_at_marker(r);
                    (lay(&left, size, upright, m), lay(&right, size, upright, m))
                })
                .collect();
            let left_width = parts.iter().map(|(l, _)| l.width).fold(0.0, f32::max);
            let gap = size * 0.3;
            let heights: Vec<(f32, f32)> = parts.iter().map(|(l, r)| (l.ascent.max(r.ascent).max(size * 0.7), l.descent.max(r.descent).max(size * 0.2))).collect();
            let total: f32 = heights.iter().map(|(a, d)| a + d).sum::<f32>() + gap * parts.len().saturating_sub(1) as f32;
            let mut y = -(total / 2.0) - size * mx().axis;
            let mut out = MathBox::default();
            let mut width = left_width;
            for (i, (l, r)) in parts.into_iter().enumerate() {
                let (a, d) = heights[i];
                y += a;
                let lw = l.width;
                let rw = r.width;
                out.put(l, left_width - lw, y);
                out.put(r, left_width, y);
                width = width.max(left_width + rw);
                y += d + gap;
            }
            out.width = width;
            out
        }
        Math::Matrix(rows) => {
            let cells: Vec<Vec<MathBox>> = rows.iter().map(|r| r.iter().map(|c| lay(c, size, upright, m)).collect()).collect();
            let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
            let widths: Vec<f32> = (0..columns).map(|c| cells.iter().filter_map(|r| r.get(c)).map(|b| b.width).fold(0.0, f32::max)).collect();
            let col_gap = size;
            let mut out = MathBox::default();
            // Each row's height above and below its baseline, and the baselines' distances (at least 1.167 of the size).
            let heights: Vec<(f32, f32)> = cells.iter().map(|r| (r.iter().map(|b| b.ascent).fold(size * 0.68, f32::max), r.iter().map(|b| b.descent).fold(0.0, f32::max))).collect();
            let pitches: Vec<f32> = (1..cells.len()).map(|i| (size * 1.167).max(heights[i - 1].1 + heights[i].0 + size * 0.17)).collect();
            let total = heights.first().map_or(0.0, |h| h.0) + pitches.iter().sum::<f32>() + heights.last().map_or(0.0, |h| h.1);
            // Centred on the axis.
            let mut y = -(total / 2.0) - size * mx().axis + heights.first().map_or(0.0, |h| h.0);
            for (r, row) in cells.into_iter().enumerate() {
                if r > 0 {
                    y += pitches[r - 1];
                }
                let mut x = 0.0;
                for (c, cell) in row.into_iter().enumerate() {
                    let cw = cell.width;
                    out.put(cell, x + (widths[c] - cw) / 2.0, y);
                    x += widths[c] + col_gap;
                }
            }
            out.width = widths.iter().sum::<f32>() + col_gap * columns.saturating_sub(1) as f32;
            out
        }
    }
}

/// A row of an equation array split at its `&` (the part before it and the part after; all of it before when there is none).
fn split_at_marker(row: &Math) -> (Math, Math) {
    let items: Vec<Math> = match row {
        Math::Row(v) => v.clone(),
        other => vec![other.clone()],
    };
    for (i, item) in items.iter().enumerate() {
        if let Math::Text(t) = item
            && let Some((before, after)) = t.split_once('&')
        {
            let mut left: Vec<Math> = items[..i].to_vec();
            if !before.is_empty() {
                left.push(Math::Text(before.to_string()));
            }
            let mut right: Vec<Math> = Vec::new();
            if !after.is_empty() {
                right.push(Math::Text(after.to_string()));
            }
            right.extend(items[i + 1..].iter().cloned());
            return (Math::Row(left), Math::Row(right));
        }
    }
    (row.clone(), Math::Row(Vec::new()))
}

fn lay_text(t: &str, size: f32, upright: bool, m: Measure, after_operand: bool) -> MathBox {
    // Letters are italic; digits, operators and the rest upright.
    let mut items = Vec::new();
    let mut run = String::new();
    let mut italic = false;
    let flush = |run: &mut String, italic: bool, items: &mut Vec<MathBox>, m: Measure| {
        if !run.is_empty() {
            if italic {
                let shown: String = run.chars().map(math_italic).collect();
                let mut boxed = text_box_as(&shown, run, size, false, m);
                // The slant of the last letter leans over what comes next (the italic correction).
                boxed.width += size * run.chars().last().map_or(0.0, |c| mx().italic_correction(math_italic(c)));
                items.push(boxed);
            } else {
                items.push(text_box(run, size, false, m));
            }
            run.clear();
        }
    };
    // Whether what came before can be an operand: then an operator is binary (with space round it), else it is a sign.
    let mut operand = after_operand;
    for c in t.chars() {
        let it = !upright && c.is_alphabetic() && c.is_ascii_alphabetic() || (!upright && matches!(c, 'α'..='ω' | 'ϕ' | 'ϑ' | 'ϵ'));
        if it != italic {
            flush(&mut run, italic, &mut items, m);
            italic = it;
        }
        if c == '\u{a0}' {
            // A no-break space typed in a formula is as wide as the size (measured on PowerPoint's).
            flush(&mut run, italic, &mut items, m);
            items.push(MathBox { width: size, ..Default::default() });
            continue;
        }
        if is_operator(c) {
            flush(&mut run, italic, &mut items, m);
            items.push(text_box(&c.to_string(), size, false, m));
            // Space around a binary operator or a relation, none round a sign.
            let last = items.pop().expect("the operator");
            let relation = matches!(c, '=' | '≠' | '≈' | '≤' | '≥' | '<' | '>' | '→' | '←' | '↔' | '⇒' | '⇔' | '∈' | '∉' | '⊂' | '⊆' | '∝' | '≡');
            let sign = !operand && matches!(c, '+' | '−' | '-' | '±' | '∓');
            let pad = if sign && !relation || SCRIPTED.with(std::cell::Cell::get) { 0.0 } else if relation { size * REL_PAD } else { size * OP_PAD };
            items.push(MathBox { width: last.width + 2.0 * pad, ascent: last.ascent, descent: last.descent, pieces: last.shifted(pad, 0.0) });
            operand = false;
        } else {
            run.push(c);
            operand = true;
        }
    }
    flush(&mut run, italic, &mut items, m);
    lay_row(items, size)
}

fn lay_row(items: Vec<MathBox>, _size: f32) -> MathBox {
    let mut out = MathBox::default();
    let mut x = 0.0;
    for b in items {
        let w = b.width;
        out.put(b, x, 0.0);
        x += w;
    }
    out.width = x;
    out
}

/// A bracket pair around `inner`, drawn tall enough for it.
fn brackets(inner: MathBox, open: &str, close: &str, size: f32, m: Measure) -> MathBox {
    // What the brackets span is kept symmetric about the maths axis (the greater reach above or below it, twice).
    let axis = size * mx().axis;
    let height = 2.0 * (inner.ascent - axis).max(inner.descent + axis);
    // A bracket grows when what it holds is higher than the font's `DelimitedSubFormulaMinHeight`: then it is the font's
    // variant that is the first at least as high as what it holds.
    let own = mx().ink('(').map_or(0.93, |(below, above)| below + above);
    let grown = height / (size * own);
    let scale = if height > size * mx().delimited_min { open.chars().next().and_then(|c| mx().delimiter_scale(c, height / size)).unwrap_or(grown) } else { 1.0 };
    let bs = size * scale;
    let mut out = MathBox::default();
    let mut x = 0.0;
    let (top, bottom) = (-inner.ascent, inner.descent);
    let place = |glyph: &str, out: &mut MathBox, x: &mut f32, m: Measure| {
        if glyph.is_empty() {
            return;
        }
        // A tall square bracket is drawn as a stem with two short serifs, as the maths font's stretched one is (a scaled glyph
        // would be heavy and wide).
        if scale > 1.2 && (glyph == "[" || glyph == "]") {
            let (stem, serif, w) = (size * 0.11, size * 0.055, size * 0.33);
            let (stem_x, serif_x) = if glyph == "[" { (*x + size * 0.11, *x + size * 0.11) } else { (*x + w - size * 0.11 - stem, *x + w - size * 0.11 - size * 0.22) };
            out.pieces.push(Piece::Rule { x: stem_x, y: top, w: stem, h: bottom - top });
            out.pieces.push(Piece::Rule { x: serif_x, y: top, w: size * 0.22, h: serif });
            out.pieces.push(Piece::Rule { x: serif_x, y: bottom - serif, w: size * 0.22, h: serif });
            out.ascent = out.ascent.max(-top);
            out.descent = out.descent.max(bottom);
            *x += w;
            return;
        }
        let g = text_box(glyph, bs, false, m);
        let w = g.width;
        // The glyph's ink centred on the axis.
        let ink_centre = glyph.chars().next().and_then(|c| mx().ink(c)).map_or(axis / bs.max(1.0), |(below, above)| (above - below) / 2.0);
        out.put(g, *x, -axis + ink_centre * bs);
        *x += w;
    };
    place(open, &mut out, &mut x, m);
    let iw = inner.width;
    out.put(inner, x, 0.0);
    x += iw;
    place(close, &mut out, &mut x, m);
    out.width = x;
    out
}

/// Draws `boxed` with its baseline at `origin` (the left end).
pub(crate) fn paint(boxed: &MathBox, origin: Point<Pixels>, color: Hsla, font: &Font, window: &mut Window, cx: &mut App) {
    for piece in &boxed.pieces {
        match piece {
            Piece::Text { x, y, text, size, italic } => {
                let mut f = font.clone();
                if *italic {
                    f.style = FontStyle::Italic;
                }
                let run = TextRun { len: text.len(), font: f, color, background_color: None, underline: None, strikethrough: None };
                let shaped = window.text_system().shape_line(text.clone().into(), px(*size), &[run], None);
                let at = point(origin.x + px(*x), origin.y + px(*y) - shaped.ascent);
                let _ = shaped.paint(at, px(*size * 1.2), gpui_kit::TextAlign::Left, None, window, cx);
            }
            Piece::Rule { x, y, w, h } => {
                window.paint_quad(fill(Bounds::new(point(origin.x + px(*x), origin.y + px(*y)), size(px(*w), px(*h))), color));
            }
            Piece::Line { x0, y0, x1, y1, t } => {
                let mut path = Path::new(point(origin.x + px(*x0), origin.y + px(*y0)));
                // A thin parallelogram along the line.
                let (dx, dy) = (x1 - x0, y1 - y0);
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let (nx, ny) = (-dy / len * t / 2.0, dx / len * t / 2.0);
                let p = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
                path.line_to(p(x1 + nx, y1 + ny));
                path.line_to(p(x1 - nx, y1 - ny));
                path.line_to(p(x0 - nx, y0 - ny));
                path.line_to(p(x0 + nx, y0 + ny));
                window.paint_path(path, color);
            }
        }
    }
}

/// A measure that uses the window's text system with `font`.
pub(crate) fn window_measure<'a>(font: Font, window: &'a mut Window) -> impl FnMut(&str, f32, bool) -> (f32, f32, f32) + 'a {
    move |text, size, italic| {
        let mut f = font.clone();
        if italic {
            f.style = FontStyle::Italic;
        }
        let run = TextRun { len: text.len(), font: f, color: black(), background_color: None, underline: None, strikethrough: None };
        let shaped = window.text_system().shape_line(text.to_string().into(), px(size), &[run], None);
        (f32::from(shaped.width), f32::from(shaped.ascent), f32::from(shaped.descent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn t(s: &str) -> Box<Math> {
        Box::new(Math::Text(s.to_string()))
    }

    fn fake(text: &str, size: f32, _italic: bool) -> (f32, f32, f32) {
        (text.chars().count() as f32 * size * 0.5, size * 0.8, size * 0.2)
    }

    #[test]
    fn a_fraction_has_its_numerator_above_and_denominator_below_the_bar() {
        let b = typeset(&Math::Frac { num: t("a"), den: t("b"), kind: FracKind::Bar }, 20.0, &mut fake);
        let y_of = |s: &str| b.pieces.iter().find_map(|p| match p {
            Piece::Text { text, y, .. } if text == s => Some(*y),
            _ => None,
        }).unwrap();
        let bar = b.pieces.iter().find_map(|p| match p { Piece::Rule { y, .. } => Some(*y), _ => None }).unwrap();
        let (a, b2) = (math_italic('a').to_string(), math_italic('b').to_string());
        assert!(y_of(&a) < bar && bar < y_of(&b2) - 20.0 * 0.6, "num {} bar {} den {}", y_of(&a), bar, y_of(&b2));
        assert!(b.ascent > 0.0 && b.descent > 0.0);
    }

    #[test]
    fn a_superscript_is_smaller_and_raised() {
        let b = typeset(&Math::Script { base: t("x"), sub: None, sup: Some(t("2")) }, 20.0, &mut fake);
        let sup = b.pieces.iter().find_map(|p| match p { Piece::Text { text, y, size, .. } if text == "2" => Some((*y, *size)), _ => None }).unwrap();
        assert!(sup.0 < 0.0 && sup.1 < 20.0);
        assert!(b.width > 10.0);
    }

    #[test]
    fn a_row_puts_its_items_side_by_side_with_space_round_an_operator() {
        let b = typeset(&Math::Text("a=b".into()), 20.0, &mut fake);
        // a and b (10 each, and what an italic letter leans over what follows) and = (10 + 2 * the relation's space).
        let lean_a = 20.0 * mx().italic_correction(math_italic('a'));
        let lean_b = 20.0 * mx().italic_correction(math_italic('b'));
        assert!((b.width - (30.0 + lean_a + lean_b + 2.0 * 20.0 * REL_PAD)).abs() < 0.01, "{}", b.width);
        // A sign at the start has no space round it; after something it is an operator.
        assert!((typeset(&Math::Text("−b".into()), 20.0, &mut fake).width - (20.0 + lean_b)).abs() < 0.01);
        let after = typeset(&Math::Row(vec![Math::Text("a".into()), Math::Text("−b".into())]), 20.0, &mut fake).width;
        assert!((after - (30.0 + lean_a + lean_b + 2.0 * 20.0 * OP_PAD)).abs() < 0.01, "{after}");
    }

    #[test]
    fn an_equation_array_lines_its_rows_up_at_the_marker() {
        let rows = Math::EqArray(vec![Math::Text("x&=1".into()), Math::Text("yyyy&=2".into())]);
        let b = typeset(&rows, 20.0, &mut fake);
        let eq_x: Vec<f32> = b.pieces.iter().filter_map(|p| match p { Piece::Text { x, text, .. } if text.contains('=') => Some(*x), _ => None }).collect();
        assert_eq!(eq_x.len(), 2);
        assert!((eq_x[0] - eq_x[1]).abs() < 0.01, "the = of the rows is at one place: {eq_x:?}");
    }

    #[test]
    fn a_sum_stacks_its_limits_and_a_matrix_has_its_cells_in_columns() {
        let sum = typeset(&Math::Nary { op: "∑".into(), sub: Some(t("i")), sup: Some(t("n")), body: t("x"), stacked: true }, 20.0, &mut fake);
        assert!(sum.ascent > 20.0 && sum.descent > 4.0);
        let m = typeset(&Math::Matrix(vec![vec![Math::Text("1".into()), Math::Text("2".into())], vec![Math::Text("3".into()), Math::Text("4".into())]]), 20.0, &mut fake);
        assert!(m.width > 20.0 && m.ascent + m.descent > 35.0);
    }

    #[test]
    fn a_matrix_is_centred_on_the_axis_and_a_sum_has_its_limits_clear_of_the_sign() {
        let cell = |c: &str| Math::Text(c.into());
        let m = typeset(&Math::Matrix(vec![vec![cell("1"), cell("2")], vec![cell("3"), cell("4")]]), 36.0, &mut fake);
        let ys: Vec<f32> = m.pieces.iter().filter_map(|p| match p { Piece::Text { y, .. } => Some(*y), _ => None }).collect();
        let (top, bottom) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
        assert!(((top + bottom) / 2.0 + 36.0 * mx().axis).abs() < 36.0 * 0.5, "the rows are about the axis: {top} {bottom}");
        assert!((bottom - top - 36.0 * 1.167).abs() < 0.01, "the baselines are 1.167 of the size apart");
        let sum = typeset(&Math::Nary { op: "∑".into(), sub: Some(t("k")), sup: Some(t("n")), body: t("x"), stacked: true }, 36.0, &mut fake);
        let y_of = |s: &str| sum.pieces.iter().find_map(|p| match p { Piece::Text { text, y, .. } if *text == math_italic(s.chars().next().unwrap()).to_string() => Some(*y), _ => None }).unwrap();
        // The upper limit's baseline is above the sign and the lower one's under it.
        assert!(y_of("n") < -36.0 * mx().upper_rise && y_of("k") > 36.0 * mx().lower_drop * 0.5, "{} {}", y_of("n"), y_of("k"));
    }

    #[test]
    fn a_function_name_is_upright_with_a_thin_space_and_a_no_break_space_is_as_wide_as_the_size() {
        let f = typeset(&Math::Function { name: t("sin"), arg: t("x") }, 36.0, &mut fake);
        // sin (3 characters of the fake font) + the thin space + x (one character, with what an italic letter leans over).
        let lean = 36.0 * mx().italic_correction(math_italic('x'));
        assert!((f.width - (3.0 * 36.0 * 0.5 + 36.0 * 3.0 / 18.0 + 36.0 * 0.5 + lean)).abs() < 0.01, "{}", f.width);
        let plain = |m: &MathBox| m.pieces.iter().any(|p| matches!(p, Piece::Text { text, italic: false, .. } if text == "sin"));
        assert!(plain(&f), "the name is drawn as it is, upright");
        let gap = typeset(&Math::Text("a\u{a0}b".into()), 36.0, &mut fake).width - typeset(&Math::Text("ab".into()), 36.0, &mut fake).width;
        // The no-break space is as wide as the size; the letter before it, now the end of its own italic run, leans over it (its italic correction).
        let lean_a = 36.0 * mx().italic_correction(math_italic('a'));
        assert!((gap - (36.0 + lean_a)).abs() < 0.01, "{gap}");
    }
}
